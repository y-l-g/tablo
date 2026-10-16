#[derive(tablo_core::FormInput)]
struct Rejection {
    #[form(placeholder = "reason", colour = "red")]
    reason: String,
}

fn main() {}
