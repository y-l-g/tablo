fn main() {
    // A row's author id is a `Uuid`, not a tenant: it cannot become the request tenant.
    let author_id = uuid::Uuid::nil();
    let _ = tablo_core::Tenant(author_id);
}
