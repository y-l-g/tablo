#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns, tablo_core::ModelForm)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[form(blnk = "")]
    title: String,
}

fn main() {}
