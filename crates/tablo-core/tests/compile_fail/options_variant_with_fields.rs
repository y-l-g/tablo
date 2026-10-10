#[derive(Debug, Clone, PartialEq, Eq, tablo_core::Options)]
enum Status {
    Draft,
    Scheduled { at: String },
}

fn main() {}
