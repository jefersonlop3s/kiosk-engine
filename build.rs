// `sqlx::migrate!` embute as migrações na compilação; sem isto o cargo não recompila
// ao adicionar um arquivo em `migrations/` e o binário roda com um esquema velho.
//
// Também grava no binário o que `kioskd version` mostra: commit, data e alvo da compilação.
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    println!("cargo:rustc-env=KIOSK_BUILD_COMMIT={}", commit());
    println!("cargo:rustc-env=KIOSK_BUILD_DATE={}", data());
    println!(
        "cargo:rustc-env=KIOSK_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
    println!(
        "cargo:rustc-env=KIOSK_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into())
    );
}

/// Commit curto de onde o binário foi compilado; `unknown` fora de um checkout do Git (por
/// exemplo, o código baixado como `.tar.gz`), e `-dirty` se havia alterações não commitadas.
fn commit() -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let Some(sha) = git(&["rev-parse", "--short=7", "HEAD"]).filter(|s| !s.is_empty()) else {
        return "unknown".into();
    };
    let sujo =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    if sujo { format!("{sha}-dirty") } else { sha }
}

/// Data UTC da compilação (`AAAA-MM-DD`). `SOURCE_DATE_EPOCH` torna a compilação reproduzível.
fn data() -> String {
    let segundos = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64)
        });
    let (a, m, d) = data_civil(segundos.div_euclid(86_400));
    format!("{a:04}-{m:02}-{d:02}")
}

/// Dias desde 1970-01-01 para (ano, mês, dia), calendário gregoriano (algoritmo de Howard Hinnant).
fn data_civil(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
