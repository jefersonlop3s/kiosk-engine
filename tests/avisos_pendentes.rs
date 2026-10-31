//! E-004 F3, etapa 4 (GC-17): a fila de avisos que a bandeja ainda não confirmou.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiosk_engine::agente::Aviso;
use kiosk_engine::avisos_pendentes::{AvisosPendentes, CONFIRMACAO_SEGUNDOS};
use kiosk_engine::motor::Motivo;

fn seg(n: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap() + Duration::seconds(n)
}

fn aviso(sid: &str) -> Aviso {
    Aviso {
        sid: sid.into(),
        app: None,
        restante_segundos: 300,
        motivo: Motivo::LimiteDiarioTotal,
    }
}

#[test]
fn aviso_confirmado_sai_da_fila_sem_reserva() {
    let mut f = AvisosPendentes::default();
    f.registrar(aviso("A"), seg(0));
    assert!(f.vencidos(seg(1), |sid| sid == "A").is_empty());
    assert!(f.is_empty());
}

#[test]
fn aviso_sem_confirmacao_so_vence_depois_do_prazo() {
    let mut f = AvisosPendentes::default();
    f.registrar(aviso("A"), seg(0));
    assert!(
        f.vencidos(seg(CONFIRMACAO_SEGUNDOS - 1), |_| false)
            .is_empty()
    );
    assert_eq!(f.len(), 1);
    let v = f.vencidos(seg(CONFIRMACAO_SEGUNDOS), |_| false);
    assert_eq!(v, [aviso("A")]);
    assert!(f.is_empty(), "vence uma vez só");
}

#[test]
fn a_confirmacao_de_uma_conta_nao_salva_o_aviso_de_outra() {
    let mut f = AvisosPendentes::default();
    f.registrar(aviso("A"), seg(0));
    f.registrar(aviso("B"), seg(0));
    let v = f.vencidos(seg(CONFIRMACAO_SEGUNDOS), |sid| sid == "A");
    assert_eq!(v, [aviso("B")]);
}
