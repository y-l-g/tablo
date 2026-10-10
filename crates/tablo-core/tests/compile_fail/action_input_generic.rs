#[derive(tablo_core::ActionInput)]
struct Rejection<T> {
    reason: String,
    #[allow(dead_code)]
    marker: std::marker::PhantomData<T>,
}

fn main() {}
