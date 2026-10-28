// `sqlx::migrate!` embute as migrações na compilação; sem isto o cargo não recompila
// ao adicionar um arquivo em `migrations/` e o binário roda com um esquema velho.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
