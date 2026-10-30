//! Typed requests to one resource of a mounted panel: [`ResourceClient`], the [`Submission`] a
//! form posts, and the [`Reply`] each request answers with.
//!
//! The client reads every URL from [`tablo_core::url`] and every form key from the resource's
//! [`RecordForm::fields`]: a test follows a renamed slug or form key, fails to compile on a
//! renamed field or action, and panics naming the resource when the panel serves no such page,
//! instead of asserting on a 404.

use std::{any::type_name, convert::Infallible, marker::PhantomData};

use http::{StatusCode, header::LOCATION};
use tablo_core::{Action, FormField, Notification, PanelHandle, RecordForm, Resource, url};
use topcoat::context::Cx;

use crate::{
    TestClient,
    protocol::{
        EmptyTable, ErrorKind, Group, Row, body_string, declined_message, dirty_form, empty_table,
        field_error, field_error_message, form_body, groups, input_value, notification_in,
        response_cookies, row_keys, rows, stale_form,
    },
};

/// The field enum of `R`'s record form, e.g. `PostFormField`.
pub type FieldOf<R> = <<R as Resource>::Form as RecordForm>::Field;

/// Requests to `R` in the panel [`TestClient::panel`] names, built by
/// [`TestClient::resource`].
///
/// ```rust,no_run
/// # use tablo_core::Resource;
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns, tablo_core::ModelForm)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// # struct PostResource;
/// # impl Resource for PostResource { type Model = Post; type Form = PostForm; }
/// # async fn create(client: &tablo_test::TestClient<'_>) {
/// let posts = client.resource::<PostResource>();
/// let reply = posts
///     .create()
///     .set(PostFormField::Title, "Hello")
///     .submit()
///     .await;
/// assert_eq!(reply.status(), 303);
/// assert_eq!(reply.follow().await.rows().len(), 1);
/// # }
/// ```
pub struct ResourceClient<'c, 'a, R> {
    client: &'c TestClient<'a>,
    panel: &'c PanelHandle,
    resource: PhantomData<fn() -> R>,
}

impl<'c, 'a, R: Resource> ResourceClient<'c, 'a, R> {
    pub(crate) fn new(client: &'c TestClient<'a>, panel: &'c PanelHandle) -> Self {
        Self {
            client,
            panel,
            resource: PhantomData,
        }
    }

    /// `R`'s list URL, e.g. `/admin/posts`.
    ///
    /// # Panics
    ///
    /// When the panel does not mount `R`.
    pub fn url(&self) -> String {
        resolved::<R>(url::resource::<R>(&self.cx()), "a list")
    }

    /// The list page.
    pub async fn list(&self) -> Reply<'c, 'a, FieldOf<R>> {
        self.get(self.url()).await
    }

    /// The list page with `query`, e.g. `"search=ada&page=2"`.
    pub async fn list_with(&self, query: &str) -> Reply<'c, 'a, FieldOf<R>> {
        self.get(format!("{}?{query}", self.url())).await
    }

    /// The create page.
    ///
    /// # Panics
    ///
    /// When `R` serves no create page.
    pub async fn create_page(&self) -> Reply<'c, 'a, FieldOf<R>> {
        self.get(self.create_url()).await
    }

    /// A submission of the create form, posting only the fields [`Submission::set`] names.
    ///
    /// # Panics
    ///
    /// When `R` serves no create page.
    pub fn create(&self) -> Submission<'c, 'a, FieldOf<R>> {
        Submission::new(self.client, self.create_url(), R::Form::fields())
    }

    /// `record`'s detail page.
    ///
    /// # Panics
    ///
    /// When `R` declares no detail page, or its rows have no route.
    pub async fn view(&self, record: &R::Model) -> Reply<'c, 'a, FieldOf<R>> {
        let url = resolved::<R>(url::record::<R>(&self.cx(), record), "a detail page");
        self.get(url).await
    }

    /// `record`'s edit page.
    ///
    /// # Panics
    ///
    /// When `R` has no record form, or its rows have no route.
    pub async fn edit_page(&self, record: &R::Model) -> Reply<'c, 'a, FieldOf<R>> {
        self.get(self.edit_url(record)).await
    }

    /// A submission of `record`'s edit form. It posts only the fields [`Submission::set`] names,
    /// and the edit leaves every other field as stored.
    ///
    /// # Panics
    ///
    /// When `R` has no record form, or its rows have no route.
    pub fn edit(&self, record: &R::Model) -> Submission<'c, 'a, FieldOf<R>> {
        Submission::new(self.client, self.edit_url(record), R::Form::fields())
    }

    /// A confirmed delete of `record`, as the delete dialog posts it.
    ///
    /// # Panics
    ///
    /// When `R`'s rows have no route.
    pub fn delete(&self, record: &R::Model) -> Submission<'c, 'a, Infallible> {
        let url = resolved::<R>(url::delete::<R>(&self.cx(), record), "a delete route");
        Submission::new(self.client, url, Vec::new()).set_key(CONFIRM, "1")
    }

    /// A confirmed run of the action `A` on `record`. An input the action asks for goes in with
    /// [`Submission::set_key`].
    ///
    /// # Panics
    ///
    /// When `R` does not declare `A` on a record.
    pub fn action<A: Action<R>>(&self, record: &R::Model) -> Submission<'c, 'a, Infallible> {
        let url = url::action::<R, A>(&self.cx(), record).unwrap_or_else(|| {
            panic!(
                "`{}` declares no record action `{}`",
                type_name::<R>(),
                A::NAME
            )
        });
        Submission::new(self.client, url, Vec::new()).set_key(CONFIRM, "1")
    }

    /// A confirmed run of the bulk action `A` on the rows keyed `keys`, as [`Reply::row_keys`]
    /// reads them.
    ///
    /// # Panics
    ///
    /// When `R` does not declare `A` in its bulk bar.
    pub fn bulk_action<A: Action<R>>(
        &self,
        keys: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Submission<'c, 'a, Infallible> {
        let url = url::bulk_action::<R, A>(&self.cx()).unwrap_or_else(|| {
            panic!(
                "`{}` declares no bulk action `{}`",
                type_name::<R>(),
                A::NAME
            )
        });
        let ids = keys
            .into_iter()
            .map(|key| key.as_ref().to_string())
            .collect::<Vec<_>>()
            .join(",");
        Submission::new(self.client, url, Vec::new())
            .set_key(IDS, &ids)
            .set_key(CONFIRM, "1")
    }

    /// A confirmed run of the bulk action `A` on every record the list's `query` selects, as the
    /// bulk bar's Select all matching posts it: `query` spells the list's URL query, without the
    /// leading `?`, such as `f.status=draft`.
    ///
    /// # Panics
    ///
    /// When `R` does not declare `A` in its bulk bar.
    pub fn bulk_action_matching<A: Action<R>>(
        &self,
        query: &str,
    ) -> Submission<'c, 'a, Infallible> {
        let url = url::bulk_action::<R, A>(&self.cx()).unwrap_or_else(|| {
            panic!(
                "`{}` declares no bulk action `{}`",
                type_name::<R>(),
                A::NAME
            )
        });
        Submission::new(self.client, url, Vec::new())
            .set_key(MATCHING, query)
            .set_key(CONFIRM, "1")
    }

    fn create_url(&self) -> String {
        resolved::<R>(url::create::<R>(&self.cx()), "a create page")
    }

    fn edit_url(&self, record: &R::Model) -> String {
        resolved::<R>(url::edit::<R>(&self.cx(), record), "an edit page")
    }

    fn cx(&self) -> Cx {
        self.panel.enter(&self.panel.context())
    }

    async fn get(&self, url: String) -> Reply<'c, 'a, FieldOf<R>> {
        Reply::read(self.client, self.client.get(&url).await, R::Form::fields()).await
    }
}

/// The key a confirmation dialog posts, which a delete and an action that asks first require.
const CONFIRM: &str = "confirm";

/// The key the bulk bar posts its selection in, comma-separated.
const IDS: &str = "ids";

/// The key the bulk bar posts the list's query in when it selects every matching record.
const MATCHING: &str = "matching";

/// The key the create page's Create & create another button posts.
const CREATE_ANOTHER: &str = "create_another";

fn resolved<R>(url: Option<String>, what: &str) -> String {
    url.unwrap_or_else(|| panic!("the panel serves `{}` no {what}", type_name::<R>()))
}

/// A form post being built: the pairs it sends, keyed by the form's field enum `K`.
///
/// A record form's field names its key, so [`set`](Self::set) takes `PostFormField::Title`
/// rather than `"title"`. [`set_key`](Self::set_key) posts a key no field enum names: an
/// action's input, one key of an embedded field, a repeater row's key.
#[must_use = "a submission posts nothing until `submit`"]
pub struct Submission<'c, 'a, K> {
    client: &'c TestClient<'a>,
    url: String,
    fields: Vec<FormField<K>>,
    pairs: Vec<(String, String)>,
}

impl<'c, 'a, K: PartialEq> Submission<'c, 'a, K> {
    fn new(client: &'c TestClient<'a>, url: String, fields: Vec<FormField<K>>) -> Self {
        Self {
            client,
            url,
            fields,
            pairs: Vec::new(),
        }
    }

    /// The URL the submission posts to.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Posts `value` as `field`, replacing any value set before.
    ///
    /// # Panics
    ///
    /// When `field` binds more than one key, as an embedded field does: set each with
    /// [`set_key`](Self::set_key).
    pub fn set(self, field: K, value: &str) -> Self {
        let key = single_key(&self.fields, &field).to_string();
        self.set_key(&key, value)
    }

    /// Posts `value` as one more value of `field`, as a multiple choice posts each pick.
    ///
    /// # Panics
    ///
    /// When `field` binds more than one key.
    pub fn add(mut self, field: K, value: &str) -> Self {
        let key = single_key(&self.fields, &field).to_string();
        self.pairs.push((key, value.to_string()));
        self
    }

    /// Posts `value` under `key`, replacing any value set before.
    pub fn set_key(mut self, key: &str, value: &str) -> Self {
        self.pairs.retain(|(kept, _)| kept != key);
        self.pairs.push((key.to_string(), value.to_string()));
        self
    }

    /// Posts as the create page's Create & create another button does, which lands back on a
    /// fresh create form.
    pub fn create_another(self) -> Self {
        self.set_key(CREATE_ANOTHER, "1")
    }

    /// Posts as the create page a creatable choice's dialog frames does, which lands on the page
    /// [`Reply::picked`] reads.
    pub fn picking(mut self) -> Self {
        let joint = if self.url.contains('?') { '&' } else { '?' };
        self.url = format!("{}{joint}pick=1", self.url);
        self
    }

    /// Posts the pairs with a fresh CSRF token, as [`TestClient::submit`] does.
    pub async fn submit(self) -> Reply<'c, 'a, K> {
        let pairs: Vec<(&str, &str)> = self
            .pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let response = self.client.submit(&self.url, &form_body(&pairs)).await;
        Reply::read(self.client, response, self.fields).await
    }
}

fn single_key<'f, K: PartialEq>(fields: &'f [FormField<K>], field: &K) -> &'f str {
    let field = fields
        .iter()
        .find(|candidate| candidate.field == *field)
        .expect("a field enum's variant is one of its form's fields");
    match field.keys.as_slice() {
        [key] => key,
        keys => panic!(
            "`{}` binds the keys {keys:?}: post each with `set_key`",
            field.name
        ),
    }
}

/// A response with its body read, keyed by the form's field enum `K` for the queries over its
/// form.
///
/// The queries are those of the free functions this crate exports, over the response's HTML.
pub struct Reply<'c, 'a, K> {
    client: &'c TestClient<'a>,
    fields: Vec<FormField<K>>,
    status: StatusCode,
    location: Option<String>,
    cookies: Vec<(String, String)>,
    html: String,
}

impl<'c, 'a, K: PartialEq> Reply<'c, 'a, K> {
    async fn read(
        client: &'c TestClient<'a>,
        response: http::Response<topcoat::router::Body>,
        fields: Vec<FormField<K>>,
    ) -> Self {
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let cookies = response_cookies(&response);
        Self {
            client,
            fields,
            status: response.status(),
            location,
            cookies,
            html: body_string(response).await,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The `Location` a redirect names.
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    /// The `(name, value)` pairs the response's `Set-Cookie` headers carry.
    pub fn cookies(&self) -> &[(String, String)] {
        &self.cookies
    }

    /// The flash notification the response set for the next page.
    pub fn notification(&self) -> Option<Notification> {
        notification_in(&self.cookies)
    }

    pub fn html(&self) -> &str {
        &self.html
    }

    /// Follows the redirect, carrying the cookies the response set, as a browser does.
    ///
    /// # Panics
    ///
    /// When the response names no `Location`.
    pub async fn follow(&self) -> Reply<'c, 'a, K>
    where
        K: Clone,
    {
        let location = self
            .location
            .as_deref()
            .unwrap_or_else(|| panic!("a {} response redirects nowhere", self.status));
        let response = self.client.cookies(&self.cookies).get(location).await;
        Reply::read(self.client, response, self.fields.clone()).await
    }

    /// See [`picked`](crate::picked).
    pub fn picked(&self) -> Option<crate::Picked> {
        crate::picked(&self.html)
    }

    /// See [`create_option_links`](crate::create_option_links).
    pub fn create_option_links(&self) -> Vec<crate::CreateOptionLink> {
        crate::create_option_links(&self.html)
    }

    /// See [`rows`](crate::rows).
    pub fn rows(&self) -> Vec<Row> {
        rows(&self.html)
    }

    /// See [`row_keys`](crate::row_keys).
    pub fn row_keys(&self) -> Vec<String> {
        row_keys(&self.html)
    }

    /// See [`groups`](crate::groups).
    pub fn groups(&self) -> Vec<Group> {
        groups(&self.html)
    }

    /// See [`empty_table`](crate::empty_table).
    pub fn empty_table(&self) -> Option<EmptyTable> {
        empty_table(&self.html)
    }

    /// See [`tabs`](crate::tabs).
    pub fn tabs(&self) -> Vec<crate::Tab> {
        crate::tabs(&self.html)
    }

    /// See [`sections`](crate::sections).
    pub fn sections(&self) -> Vec<crate::Section> {
        crate::sections(&self.html)
    }

    /// Whether the edit form refused the save as stale; see [`stale_form`](crate::stale_form).
    pub fn stale(&self) -> bool {
        stale_form(&self.html)
    }

    /// The message a record fn declined the save with; see
    /// [`declined_message`](crate::declined_message).
    pub fn declined(&self) -> Option<String> {
        declined_message(&self.html)
    }

    /// Whether the form renders dirty, guarding what it holds against leaving the page; see
    /// [`dirty_form`](crate::dirty_form).
    pub fn dirty(&self) -> bool {
        dirty_form(&self.html)
    }

    /// Why `field`'s error slot renders, read under its first key; see
    /// [`field_error`](crate::field_error).
    pub fn field_error(&self, field: K) -> Option<ErrorKind> {
        field_error(&self.html, self.first_key(&field))
    }

    /// The message `field`'s error slot renders; see
    /// [`field_error_message`](crate::field_error_message).
    pub fn field_error_message(&self, field: K) -> Option<String> {
        field_error_message(&self.html, self.first_key(&field))
    }

    /// The value `field`'s input renders, as an edit page fills it; see
    /// [`input_value`](crate::input_value).
    pub fn input_value(&self, field: K) -> Option<String> {
        input_value(&self.html, self.first_key(&field))
    }

    fn first_key(&self, field: &K) -> &str {
        self.fields
            .iter()
            .find(|candidate| candidate.field == *field)
            .and_then(|field| field.keys.first())
            .expect("a field enum's variant binds a key of its form")
    }
}
