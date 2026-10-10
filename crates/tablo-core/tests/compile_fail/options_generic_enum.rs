#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
enum Status<T> {
    Draft,
    Published,
    #[allow(dead_code)]
    Archived(std::marker::PhantomData<T>),
}

fn main() {}
