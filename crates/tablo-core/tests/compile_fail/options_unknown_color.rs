#[derive(Debug, Clone, PartialEq, Eq, tablo_core::Options)]
enum Status {
    #[option(color = "red")]
    Failed,
}

fn main() {}
