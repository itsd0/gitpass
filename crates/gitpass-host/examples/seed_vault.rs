//! Dev helper: create a demo vault for manual host testing.
//! `cargo run -p gitpass-host --example seed_vault -- <dir> <password>` (writes <dir>/secret.key)
use gitpass_core::store::{self, Item, Session};
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    std::fs::create_dir_all(&root)?;
    let sk = store::init(&root, &a[2], Default::default())?;
    std::fs::write(root.join("secret.key"), format!("{}\n", sk.encode()))?;
    let s = Session::unlock(&root, &a[2], &sk)?;
    let vid = s.create_vault("personal")?;
    let it = Item { title: "GitHub".into(), username: Some("leeky".into()), password: Some("s3cret".into()),
        url: Some("https://github.com".into()), notes: None, totp: Some("JBSWY3DPEHPK3PXP".into()), updated_at: store::now() };
    println!("{vid}:{}", s.add_item(&vid, &it)?);
    Ok(())
}
