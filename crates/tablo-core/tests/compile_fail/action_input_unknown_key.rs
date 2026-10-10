#[derive(tablo_core::ActionInput)]
struct Rejection {
    #[form(placeholder = "reason", colour = "red")]
    reason: String,
}

fn main() {}
