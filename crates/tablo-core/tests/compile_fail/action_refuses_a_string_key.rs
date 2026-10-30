use tablo_core::{Action, FormInput, NoForm, Outcome, Resource, WriteError};
use topcoat::{Result, context::Cx};

#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
}

struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = NoForm<Post>;
}

#[derive(FormInput)]
struct Note {
    body: String,
}

struct Annotate;

impl Action<PostResource> for Annotate {
    type Input = Note;
    const NAME: &'static str = "annotate";

    async fn run(
        _: &Cx,
        _: &[Post],
        _: Note,
        _: &mut dyn toasty::Executor,
    ) -> Result<Outcome, WriteError<NoteField>> {
        // A refusal names a field of the input's enum, never a string a typo could miss.
        Err(WriteError::field("bdy", "Too short"))
    }
}

fn main() {}
