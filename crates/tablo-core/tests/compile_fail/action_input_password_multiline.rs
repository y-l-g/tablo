#[derive(tablo_core::ActionInput)]
struct Login {
    #[form(password, multiline = 4)]
    secret: String,
}

fn main() {}
