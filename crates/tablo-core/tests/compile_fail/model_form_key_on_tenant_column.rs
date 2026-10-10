#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns, tablo_core::ModelForm)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[form(optional)]
    tenant_id: tablo_core::TenantId,
    title: String,
}

fn main() {}
