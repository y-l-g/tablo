#[derive(Debug, Clone, toasty::Model)]
struct Tagged {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tags: Vec<String>,
}

#[derive(tablo_core::RecordForm)]
#[form(model = Tagged)]
struct TaggedForm {
    tags: Vec<String>,
}

fn main() {}
