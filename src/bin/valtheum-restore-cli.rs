#[path = "../archive_crypto.rs"]
#[allow(dead_code)]
mod archive_crypto;
fn main() {
    if let Err(error) = run() {
        eprintln!("복원 실패: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--license" {
        print!("{}", include_str!("../../LICENSE"));
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--licenses" {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../static/licenses.json"))?;
        println!(
            "valtheum {} — MIT",
            catalog["version"].as_str().unwrap_or("")
        );
        println!("{}", catalog["summary"].as_str().unwrap_or(""));
        for component in catalog["components"].as_array().unwrap() {
            println!(
                "{} {} | {} | {}",
                component["name"].as_str().unwrap(),
                component["version"].as_str().unwrap(),
                component["license"].as_str().unwrap(),
                component["scope"].as_str().unwrap()
            );
            for notice in component["notices"].as_array().unwrap() {
                println!(
                    "  {} -> {}",
                    notice["file"].as_str().unwrap(),
                    notice["text"].as_str().unwrap()
                );
            }
        }
        for (id, text) in catalog["texts"].as_object().unwrap() {
            println!("\n===== {id} =====\n{}", text.as_str().unwrap());
        }
        return Ok(());
    }
    anyhow::ensure!(
        args.len() == 3 || (args.len()==4 && args[3]=="--notes"),
        "사용법: valtheum-restore-cli.exe <vault.key> <암호화파일.vta/.vtv/.vti/.vtn> <복원 파일> [--notes]"
    );
    let password = zeroize::Zeroizing::new(rpassword::prompt_password("비밀번호: ")?);
    let key = archive_crypto::load_key(std::path::Path::new(&args[0]), &password)?;
    if args.len() == 4 {
        archive_crypto::image::restore_notes(
            std::path::Path::new(&args[1]),
            &key,
            std::path::Path::new(&args[2]),
            |_| Ok(()),
        )?;
        println!("메모 내보내기 완료");
        return Ok(());
    }
    let metadata = archive_crypto::restore(
        std::path::Path::new(&args[1]),
        &key,
        std::path::Path::new(&args[2]),
    )?;
    if metadata.format == "json" {
        println!("메모 JSON 복원 완료");
        return Ok(());
    }
    println!(
        "복원 완료: {} ({} bytes, SHA-256 {})",
        metadata.name, metadata.size, metadata.sha256
    );
    Ok(())
}
