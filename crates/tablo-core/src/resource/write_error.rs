//! [`WriteError`]: why a record fn or an action's run wrote nothing.

use derive_where::derive_where;

use crate::form::{FieldErrors, FormFields};

/// Why a record fn or an action's run wrote nothing: a refusal the page shows, or a failure.
///
/// Either way the handler rolls the transaction back, so whatever the fn wrote before it returned
/// is undone.
///
/// - [`Refused`](Self::Refused) renders the form again with status 200, each error under the
///   control of the field it names, as a [`validate_record`](crate::Resource::validate_record)
///   error does. A record fn keys its errors by its record form, an action's run by its input, and
///   picks a field by its name, so a renamed field fails to compile rather than render no message.
/// - [`Declined`](Self::Declined) refuses with a message that belongs to no field: a paid invoice
///   that cannot be deleted. A record form renders again with the message above its fields; a
///   delete, a restore or an action flashes it and lands back where it started.
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
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, sku: String, quantity: i64 }
/// # #[derive(Debug, toasty::Model)]
/// # struct Stock { #[key] sku: String, level: i64 }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = Order)]
/// # struct OrderForm { sku: String, quantity: i64 }
/// # struct OrderResource;
/// use tablo_core::{RecordForm, Resource, WriteError, write_create};
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
///     ) -> Result<Order, WriteError<OrderForm>> {
///         let stock = Stock::filter_by_sku(&form.sku)
///             .first()
///             .exec(&mut *ex)
///             .await?;
///         if stock.is_none_or(|stock| stock.level < form.quantity) {
///             return Err(WriteError::<OrderForm>::field(
///                 |f| f.quantity,
///                 "Not enough in stock",
///             ));
///         }
///         write_create::<Self>(cx, form, ex).await
///     }
/// }
/// ```
#[derive_where(Debug; F::Field)]
#[non_exhaustive]
pub enum WriteError<F: FormFields = String> {
    /// The submission cannot stand: render the form again with these errors.
    Refused(FieldErrors<F>),
    /// The write cannot stand, for a reason that names no field: show this message.
    Declined(crate::Text),
    /// The write failed: answer with this error and the failure toast.
    Failed(topcoat::Error),
}

impl<F: FormFields> WriteError<F> {
    /// Refuse the submission with `message` under `field`: `WriteError::field(|f| f.room, ..)`.
    pub fn field(pick: impl FnOnce(F::Keys) -> F::Field, message: impl Into<crate::Text>) -> Self {
        let mut errors = FieldErrors::new();
        errors.add(pick, message);
        Self::Refused(errors)
    }

    /// Decline the write with `message`, which names no field.
    pub fn declined(message: impl Into<crate::Text>) -> Self {
        Self::Declined(message.into())
    }

    /// This error, its refusal keyed by the form key `key` gives each field.
    pub(crate) fn keyed(self, key: fn(F::Field) -> &'static str) -> WriteError {
        match self {
            Self::Refused(errors) => WriteError::Refused(errors.keyed(key)),
            Self::Declined(message) => WriteError::Declined(message),
            Self::Failed(error) => WriteError::Failed(error),
        }
    }
}

impl<F: FormFields> From<FieldErrors<F>> for WriteError<F> {
    fn from(errors: FieldErrors<F>) -> Self {
        Self::Refused(errors)
    }
}

impl<F: FormFields, E: Into<topcoat::Error>> From<E> for WriteError<F> {
    fn from(error: E) -> Self {
        Self::Failed(error.into())
    }
}
