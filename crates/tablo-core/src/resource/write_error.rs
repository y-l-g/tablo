//! [`WriteError`]: why a record fn or an action's run wrote nothing.

use crate::form::FieldErrors;

/// Why a record fn or an action's run wrote nothing: a refusal the form renders, or a failure.
///
/// Either way the handler rolls the transaction back, so whatever the fn wrote before it returned
/// is undone.
///
/// - [`Refused`](Self::Refused) renders the form again with status 200, each error under the
///   control of the field it names, as a [`validate_record`](crate::Resource::validate_record)
///   error does. A record fn keys its errors by the record form's field enum, an action's run by
///   its input's form keys.
/// - [`Failed`](Self::Failed) answers as a failed write does: a record fn's collision on a unique
///   index renders under the field's control, a save another save overtook renders the stale form,
///   and any other error flashes the failure toast and answers with the error.
///
/// A refusal is the answer to a rule that reads the database: a stock level, a quota, a
/// uniqueness no index models. A rule the write itself enforces holds against concurrent writes:
/// an update filtered on the stock it needs that matches no row refuses the very write it would
/// have made.
///
/// `?` turns any error a [`topcoat::Error`] takes into [`Failed`](Self::Failed), and a
/// [`FieldErrors`] into [`Refused`](Self::Refused):
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, sku: String, quantity: i64 }
/// # #[derive(Debug, toasty::Model)]
/// # struct Stock { #[key] sku: String, level: i64 }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = Order)]
/// # struct OrderForm { sku: String, quantity: i64 }
/// # struct OrderResource;
/// use tablo_core::{FieldErrors, RecordForm, Resource, WriteError, write_create};
/// use topcoat::context::Cx;
///
/// impl Resource for OrderResource {
///     type Model = Order;
///     type Form = OrderForm;
///
///     async fn create_record(
///         cx: &Cx,
///         form: OrderForm,
///         ex: &mut dyn toasty::Executor,
///     ) -> Result<Order, WriteError<OrderFormField>> {
///         let stock = Stock::filter_by_sku(&form.sku)
///             .first()
///             .exec(&mut *ex)
///             .await?;
///         if stock.is_none_or(|stock| stock.level < form.quantity) {
///             let mut errors = FieldErrors::new();
///             errors.add(OrderFormField::Quantity, "Not enough in stock");
///             return Err(errors.into());
///         }
///         Ok(write_create::<Self>(cx, form, ex).await?)
///     }
/// }
/// ```
#[derive(Debug)]
pub enum WriteError<K = String> {
    /// The submission cannot stand: render the form again with these errors.
    Refused(FieldErrors<K>),
    /// The write failed: answer with this error and the failure toast.
    Failed(topcoat::Error),
}

impl<K> WriteError<K> {
    /// Refuse the submission with `message` under `key`.
    pub fn field(key: impl Into<K>, message: impl Into<String>) -> Self {
        let mut errors = FieldErrors::new();
        errors.add(key, message);
        Self::Refused(errors)
    }
}

impl<K> From<FieldErrors<K>> for WriteError<K> {
    fn from(errors: FieldErrors<K>) -> Self {
        Self::Refused(errors)
    }
}

impl<K, E: Into<topcoat::Error>> From<E> for WriteError<K> {
    fn from(error: E) -> Self {
        Self::Failed(error.into())
    }
}
