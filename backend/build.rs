//! As migrações entram no binário em tempo de compilação (`sqlx::migrate!`):
//! migração nova sem mudança no código não recompilava, e o binário rodava sem
//! ela. Isto recompila quando a pasta muda (o que a documentação do sqlx pede).
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
