//! The page a panel answers a failed page load with: the error inside the shell, in place of the
//! router's bare text.
//!
//! The panel gate turns a 404, a 403 or a 500 into an internal rewrite to `{prefix}/-/error`,
//! carrying the [`PanelError`]; that page renders in the panel's layout like any other, under the
//! browser's own URL. A catch-all route under the prefix sends a path no route serves through the
//! gate too, so it signs in first and then shows the 404 page.

use topcoat::{
    context::{Cx, try_context},
    icon::icon,
    router::{
        Body, RouteFuture,
        error::{
            BadRequestError, ContentTooLargeError, ForbiddenError, InternalServerError,
            MethodNotAllowedError, NotFoundError, RedirectError, RewriteError, SeeOther,
            ServiceUnavailableError, TooManyRequestsError, UnauthorizedError, not_found, rewrite,
        },
    },
    view::{BoxView, ViewExt, view},
};

use super::gate::panel_prefix;

/// The path of the error page under a panel's prefix.
pub(crate) const ERROR_SEGMENT: &str = "-/error";

/// The failure a panel page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelError {
    /// 404: nothing at the path, or a record the request may not know of.
    NotFound,
    /// 403: the user may not open the page.
    Forbidden,
    /// 500: the page failed.
    Failed,
}

impl PanelError {
    /// The page `error` shows, or `None` when it answers with its own response: a redirect, a
    /// rewrite, or a status the browser acts on (400, 401, 405, 413, 429 and 503). Any error the
    /// router knows nothing of answers 500, so it shows [`Failed`](Self::Failed).
    pub(crate) fn of(error: &topcoat::Error) -> Option<Self> {
        if error.is::<NotFoundError>() {
            return Some(Self::NotFound);
        }
        if error.is::<ForbiddenError>() {
            return Some(Self::Forbidden);
        }
        if error.is::<InternalServerError>() {
            return Some(Self::Failed);
        }
        let answered = error.is::<RedirectError>()
            || error.is::<SeeOther>()
            || error.is::<RewriteError>()
            || error.is::<BadRequestError>()
            || error.is::<UnauthorizedError>()
            || error.is::<MethodNotAllowedError>()
            || error.is::<ContentTooLargeError>()
            || error.is::<TooManyRequestsError>()
            || error.is::<ServiceUnavailableError>();
        (!answered).then_some(Self::Failed)
    }

    pub(crate) fn status(self) -> http::StatusCode {
        match self {
            Self::NotFound => http::StatusCode::NOT_FOUND,
            Self::Forbidden => http::StatusCode::FORBIDDEN,
            Self::Failed => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The page's heading, and the trail's last step.
    pub(crate) fn title(self) -> crate::Text {
        crate::Text::message(match self {
            Self::NotFound => "tablo-error-not-found",
            Self::Forbidden => "tablo-error-forbidden",
            Self::Failed => "tablo-error-failed",
        })
    }

    fn description(self) -> crate::Text {
        crate::Text::message(match self {
            Self::NotFound => "tablo-error-not-found-description",
            Self::Forbidden => "tablo-error-forbidden-description",
            Self::Failed => "tablo-error-failed-description",
        })
    }
}

/// The error the request answers with: for a page load that failed with a [`PanelError`], a
/// rewrite to the panel's error page; else `error` itself. The error page's own failure passes
/// through, so it answers the router's bare text instead of rewriting again.
pub(crate) fn answer(cx: &Cx, error: topcoat::Error) -> topcoat::Error {
    if try_context::<PanelError>(cx).is_some() || !crate::auth::page_load(cx) {
        return error;
    }
    let Some(page) = PanelError::of(&error) else {
        return error;
    };
    // The rewrite answers in the failure's place, so a layer outside the panel sees only the
    // rewrite: the cause is logged here or nowhere.
    if page == PanelError::Failed {
        tracing::error!(error = %error, "panel page failed; showing the error page");
    }
    rewrite(
        format!("{}/{ERROR_SEGMENT}", panel_prefix(cx)),
        Body::empty(),
    )
    .with(page)
    .into()
}

/// The request's failure, while the error page renders it.
pub(crate) fn current_error(cx: &Cx) -> Option<PanelError> {
    try_context::<PanelError>(cx).copied()
}

/// `GET {prefix}/-/error`: the failure the rewrite carries, else a 404 for a visit to the path
/// itself.
pub(crate) fn error_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    let error = current_error(cx).unwrap_or(PanelError::NotFound);
    let code = error.status().as_u16().to_string();
    let home = crate::navigation::runtime_link(cx, &panel_prefix(cx));
    let button =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    view! {
        cx =>
        (error.status())
        tablo_ui::page(
            attrs: topcoat::view::attributes! { class="flex-1 justify-center" },
            <div
                class="flex flex-col items-center gap-3 py-16 text-center"
                data-panel-error=(code.clone())
            >
                <p class="text-sm font-semibold text-muted-foreground">(code)</p>
                <h1 class="text-2xl font-semibold tracking-tight text-foreground">
                    (error.title().localize(cx))
                </h1>
                <p class="max-w-md text-sm text-muted-foreground">
                    (error.description().localize(cx))
                </p>
                <a (home) class=(button) data-panel-error-home="">
                    icon(data: tablo_ui::icons::ARROW_LEFT)
                    (crate::t!(cx, "tablo-error-home"))
                </a>
            </div>
        )
    }
    .boxed()
}

/// `{prefix}/{*rest}`, any method: a path no other route serves answers 404 behind the gate.
pub(crate) fn unmatched(_cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async { Err(not_found().into()) })
}

#[cfg(test)]
mod tests;
