#[derive(tablo_core::FormInput)]
struct Login {
    #[form(password, multiline = 4)]
    secret: String,
}

fn main() {}
