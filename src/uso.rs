//! Contabilidade de uso (E-003 F2): divide um intervalo de uso pelos dias locais.
//! Puro e independente de SO; o fuso entra como parâmetro (`chrono::Local` no agente).

use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};

/// Maior intervalo aceito numa única contabilização. Acima disso (ou negativo) o relógio
/// é considerado suspeito: vira evento `TAMPER_SUSPECTED`, nunca ajuste silencioso.
pub const MAX_INTERVALO_SEGUNDOS: i64 = 600;

/// Primeiro instante do dia local `dia`. Se a meia-noite não existe (início de horário de verão
/// à meia-noite), vale o primeiro instante existente do dia; se ocorre duas vezes, a primeira.
fn inicio_do_dia<Tz: TimeZone>(tz: &Tz, dia: NaiveDate) -> Result<DateTime<Utc>, String> {
    for hora in 0..4 {
        let naive = dia.and_hms_opt(hora, 0, 0).ok_or("hora inválida")?;
        match tz.from_local_datetime(&naive) {
            LocalResult::Single(d) => return Ok(d.with_timezone(&Utc)),
            LocalResult::Ambiguous(primeira, _) => return Ok(primeira.with_timezone(&Utc)),
            LocalResult::None => continue,
        }
    }
    Err(format!(
        "não foi possível determinar o início do dia local {dia}"
    ))
}

/// Divide `[inicio, fim)` pelos dias locais, em segundos reais decorridos (a duração do dia
/// pode ter 23 ou 25 horas na virada do horário de verão).
pub fn dividir_por_dia_local<Tz: TimeZone>(
    tz: &Tz,
    inicio: DateTime<Utc>,
    fim: DateTime<Utc>,
) -> Result<Vec<(NaiveDate, u64)>, String> {
    if fim < inicio {
        return Err(format!("intervalo negativo: {inicio} -> {fim}"));
    }
    let mut partes = Vec::new();
    let mut atual = inicio;
    while atual < fim {
        let dia = atual.with_timezone(tz).date_naive();
        let proximo_dia = dia
            .checked_add_signed(Duration::days(1))
            .ok_or("data fora do intervalo")?;
        let limite = inicio_do_dia(tz, proximo_dia)?;
        if limite <= atual {
            return Err(format!(
                "fuso inconsistente em {dia}: fim do dia ({limite}) não passa de {atual}"
            ));
        }
        let corte = limite.min(fim);
        partes.push((dia, (corte - atual).num_seconds() as u64));
        atual = corte;
    }
    Ok(partes)
}
