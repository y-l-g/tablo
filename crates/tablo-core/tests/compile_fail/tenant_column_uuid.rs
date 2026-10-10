#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    tenant_id: tablo_core::TenantId,
    author_id: uuid::Uuid,
}

fn main() {
    // `author_id` is a `Uuid`: the lens names no tenant column.
    let _ = tablo_core::Tenancy::column(Post::fields().author_id());
}
