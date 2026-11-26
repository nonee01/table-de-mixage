// Nom et version du produit dans l'exe Windows (SignPath les vérifie avant de signer).
// Seulement quand on construit sous Windows : depuis Linux (cargo-xwin) il n'y a pas de compilateur de ressources.
fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Table de mixage")
            .set("FileDescription", "Table de mixage du TP de traitement du signal")
            .set("CompanyName", "nonee01")
            .set("LegalCopyright", "MIT, nonee01");
        res.compile().expect("ressources Windows");
    }
}
