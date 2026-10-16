#[derive(tablo_core::FormInput)]
struct Rejection<T> {
    reason: String,
    marker: std::marker::PhantomData<T>,
}

fn main() {}
