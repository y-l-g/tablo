#[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    email: String,
}

#[derive(tablo_core::RecordForm)]
#[form(model = User)]
struct UserForm {
    name: String,
}

fn main() {
    let _form: tablo_core::Schema<UserForm> = tablo_core::Schema::new((
        UserForm::controls().name,
        tablo_core::Field::text(User::fields().email()),
    ));
}
