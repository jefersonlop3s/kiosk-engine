//! Núcleo do agente (E-004), independente do SO: a cada tick contabiliza o uso, consulta o motor
//! e age — avisa nos marcos de 5 e 1 min, encerra apps, bloqueia a sessão — registrando tudo no
//! log forense. O acesso ao SO fica atrás de [`Sistema`]; o adaptador Win32 (ADR-003/004) é
//! trabalho futuro, e os testes usam um adaptador falso.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::database_schema::*;
use crate::db_manager::{EventContext, KioskDbManager};
use crate::motor::{AVISOS_SEGUNDOS, Decisao, Instante, Motivo};

/// Intervalo mínimo entre dois bloqueios de sessão do mesmo SID.
pub const REBLOQUEIO_SEGUNDOS: i64 = 30;
/// Bloqueios desfeitos pela criança antes de o agente passar ao logoff (decisão do dono, 2026-10-28).
pub const MAX_BLOQUEIOS_ANTES_DO_LOGOFF: u32 = 2;
/// Tentativas de encerrar o mesmo processo antes de desistir e registrar `TAMPER_SUSPECTED`.
pub const MAX_TENTATIVAS_ENCERRAR: u32 = 3;
/// Ticks que um processo encerrado com sucesso pode continuar na lista antes de nova tentativa.
const TICKS_DE_GRACA_APOS_ENCERRAR: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessoVisto {
    pub pid: u32,
    pub process_name: String,
    /// SID do dono do processo.
    pub sid: String,
}

/// Pedido de aviso à UI da sessão (a UI formata o texto em pt-BR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aviso {
    pub sid: String,
    /// Nome do executável; `None` para avisos de sessão.
    pub app: Option<String>,
    pub restante_segundos: u64,
    pub motivo: Motivo,
}

/// Tudo o que o agente precisa do sistema operacional.
pub trait Sistema {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String>;
    fn encerrar(&mut self, pid: u32) -> Result<(), String>;
    fn bloquear_sessao(&mut self, sid: &str) -> Result<(), String>;
    fn avisar(&mut self, aviso: &Aviso) -> Result<(), String>;
    /// SIDs das sessões interativas ativas (logadas e desbloqueadas) — ADR-003.
    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String>;
    /// Encerra a sessão (logoff) quando o bloqueio simples não basta.
    fn encerrar_sessao(&mut self, sid: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acao {
    Avisou {
        sid: String,
        app: Option<String>,
        restante_segundos: u64,
        motivo: Motivo,
    },
    Encerrou {
        pid: u32,
        app: String,
        motivo: Motivo,
    },
    Bloqueou {
        sid: String,
        motivo: Motivo,
    },
    /// Logoff, depois que a criança desbloqueou a sessão `MAX_BLOQUEIOS_ANTES_DO_LOGOFF` vezes.
    EncerrouSessao {
        sid: String,
        motivo: Motivo,
    },
}

/// Resultado de um tick. Um erro isolado não interrompe o enforcement do restante.
#[derive(Debug, Default)]
pub struct Relatorio {
    pub acoes: Vec<Acao>,
    pub erros: Vec<String>,
}

type ChaveAtiva = (Uuid, Option<Uuid>, Motivo);

struct GrupoApp {
    app: AppMetadata,
    procs: Vec<ProcessoVisto>,
}

struct GrupoPerfil {
    sid: String,
    /// Sessão logada e desbloqueada neste tick.
    ativa: bool,
    apps: BTreeMap<Uuid, GrupoApp>,
}

pub struct Agente<'a, S: Sistema, Tz: TimeZone> {
    db: &'a KioskDbManager,
    sistema: S,
    tz: Tz,
    ctx: EventContext,
    ultimo_tick: Option<DateTime<Utc>>,
    avisados: HashSet<(Uuid, Option<Uuid>, Motivo, u64)>,
    tentativas: HashMap<u32, u32>,
    abandonados: HashSet<u32>,
    aguardando: HashMap<u32, u8>,
    ultimo_bloqueio: HashMap<String, DateTime<Utc>>,
    ativas_anterior: HashSet<String>,
    /// Bloqueios emitidos por SID enquanto a restrição de sessão persiste.
    bloqueios: HashMap<String, u32>,
}

impl<'a, S: Sistema, Tz: TimeZone> Agente<'a, S, Tz> {
    pub fn new(db: &'a KioskDbManager, sistema: S, tz: Tz) -> Self {
        Self {
            db,
            sistema,
            tz,
            ctx: EventContext::local(),
            ultimo_tick: None,
            avisados: HashSet::new(),
            tentativas: HashMap::new(),
            abandonados: HashSet::new(),
            aguardando: HashMap::new(),
            ultimo_bloqueio: HashMap::new(),
            ativas_anterior: HashSet::new(),
            bloqueios: HashMap::new(),
        }
    }

    pub fn sistema_mut(&mut self) -> &mut S {
        &mut self.sistema
    }

    async fn evento(
        &self,
        rel: &mut Relatorio,
        perfil: Option<Uuid>,
        tipo: EventType,
        status: TransactionStatus,
        detalhe: String,
    ) {
        let e = SecurityEvent::new(
            &self.ctx.client_ip,
            ActionType::System,
            tipo,
            status,
            &detalhe,
        )
        .with_profile(perfil);
        if let Err(err) = self.db.record_event(&e).await {
            rel.erros
                .push(format!("evento {} não gravado: {err}", tipo.as_str()));
        }
    }

    /// Um ciclo do agente em `agora`.
    pub async fn tick(&mut self, agora_utc: DateTime<Utc>) -> Relatorio {
        let mut rel = Relatorio::default();
        let anterior = self.ultimo_tick.replace(agora_utc);

        let visto = match self.sistema.processos() {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("falha ao listar processos: {e}");
                self.evento(
                    &mut rel,
                    None,
                    EventType::InternalError,
                    TransactionStatus::Failure,
                    msg.clone(),
                )
                .await;
                rel.erros.push(msg);
                return rel;
            }
        };
        let sessoes = match self.sistema.sessoes_ativas() {
            Ok(x) => x,
            Err(e) => {
                let msg = format!("falha ao listar sessões: {e}");
                self.evento(
                    &mut rel,
                    None,
                    EventType::InternalError,
                    TransactionStatus::Failure,
                    msg.clone(),
                )
                .await;
                rel.erros.push(msg);
                Vec::new()
            }
        };
        let ativas: HashSet<String> = sessoes.into_iter().collect();
        let agora = Instante::em(agora_utc, &self.tz);
        let grupos = self.agrupar(&visto, &ativas, &mut rel).await;

        let presentes: HashSet<u32> = visto.iter().map(|p| p.pid).collect();
        self.tentativas.retain(|pid, _| presentes.contains(pid));
        self.abandonados.retain(|pid| presentes.contains(pid));
        self.aguardando.retain(|pid, _| presentes.contains(pid));

        let mut ativos: HashSet<ChaveAtiva> = HashSet::new();
        let mut avaliadas: HashSet<String> = HashSet::new();
        let mut bloqueando: HashSet<String> = HashSet::new();
        for (perfil_id, grupo) in &grupos {
            // 1. contabilidade do intervalo desde o tick anterior
            if let Some(ini) = anterior {
                for app_id in grupo.apps.keys() {
                    if let Err(e) = self
                        .db
                        .registrar_uso(&self.ctx, *perfil_id, *app_id, ini, agora_utc, &self.tz)
                        .await
                    {
                        rel.erros.push(format!("uso não contabilizado: {e}"));
                    }
                }
                // tempo de sessão: só conta se a sessão estava ativa no tick anterior também
                if grupo.ativa
                    && self.ativas_anterior.contains(&grupo.sid)
                    && let Err(e) = self
                        .db
                        .registrar_sessao(&self.ctx, *perfil_id, ini, agora_utc, &self.tz)
                        .await
                {
                    rel.erros
                        .push(format!("tempo de sessão não contabilizado: {e}"));
                }
            }
            // 2. sessão do perfil (uma sessão bloqueada já está contida: nada a fazer)
            if grupo.ativa {
                match self.db.decidir_para(*perfil_id, None, agora).await {
                    Ok(d) => {
                        avaliadas.insert(grupo.sid.clone());
                        if matches!(d, Decisao::BloquearSessao { .. }) {
                            bloqueando.insert(grupo.sid.clone());
                        }
                        self.aplicar_sessao(
                            &mut rel,
                            &mut ativos,
                            agora_utc,
                            *perfil_id,
                            &grupo.sid,
                            d,
                        )
                        .await
                    }
                    Err(e) => rel.erros.push(format!("decisão de sessão falhou: {e}")),
                }
            }
            // 3. cada app em execução
            for (app_id, g) in &grupo.apps {
                match self
                    .decidir_e_aplicar_app(
                        &mut rel,
                        &mut ativos,
                        *perfil_id,
                        *app_id,
                        grupo,
                        g,
                        agora,
                    )
                    .await
                {
                    Ok(()) => {}
                    Err(e) => rel.erros.push(format!("decisão de app falhou: {e}")),
                }
            }
        }
        // restrição de sessão que acabou (ex.: novo dia) zera a contagem de bloqueios
        self.bloqueios
            .retain(|sid, _| !avaliadas.contains(sid) || bloqueando.contains(sid));
        self.ativas_anterior = ativas;
        self.avisados
            .retain(|(p, a, m, _)| ativos.contains(&(*p, *a, *m)));
        rel
    }

    #[allow(clippy::too_many_arguments)]
    async fn decidir_e_aplicar_app(
        &mut self,
        rel: &mut Relatorio,
        ativos: &mut HashSet<ChaveAtiva>,
        perfil: Uuid,
        app_id: Uuid,
        grupo: &GrupoPerfil,
        g: &GrupoApp,
        agora: Instante,
    ) -> Result<(), sqlx::Error> {
        let d = self.db.decidir_app(perfil, &g.app, agora).await?;
        self.aplicar_app(rel, ativos, perfil, app_id, &grupo.sid, g, d)
            .await;
        Ok(())
    }

    /// Associa processos a perfis (por SID) e a apps (por nome). Quem não tem perfil mapeado
    /// não é controlado (ADR-002); app desconhecido é ignorado, mas mantém a sessão do perfil viva.
    async fn agrupar(
        &self,
        visto: &[ProcessoVisto],
        ativas: &HashSet<String>,
        rel: &mut Relatorio,
    ) -> BTreeMap<Uuid, GrupoPerfil> {
        let mut perfis: HashMap<String, Option<Uuid>> = HashMap::new();
        let mut apps: HashMap<String, Option<AppMetadata>> = HashMap::new();
        let mut grupos: BTreeMap<Uuid, GrupoPerfil> = BTreeMap::new();
        for p in visto {
            if !perfis.contains_key(&p.sid) {
                let id = match self.db.perfil_por_sid(&p.sid).await {
                    Ok(x) => x.map(|c| c.profile_id),
                    Err(e) => {
                        rel.erros.push(format!("perfil não consultado: {e}"));
                        continue;
                    }
                };
                perfis.insert(p.sid.clone(), id);
            }
            let Some(perfil_id) = perfis[&p.sid] else {
                continue;
            };
            let grupo = grupos.entry(perfil_id).or_insert_with(|| GrupoPerfil {
                sid: p.sid.clone(),
                ativa: ativas.contains(&p.sid),
                apps: BTreeMap::new(),
            });

            let chave = p.process_name.to_lowercase();
            if !apps.contains_key(&chave) {
                match self.db.app_por_processo(&p.process_name).await {
                    Ok(a) => {
                        apps.insert(chave.clone(), a);
                    }
                    Err(e) => {
                        rel.erros.push(format!("app não consultado: {e}"));
                        continue;
                    }
                }
            }
            if let Some(app) = &apps[&chave] {
                grupo
                    .apps
                    .entry(app.app_id)
                    .or_insert_with(|| GrupoApp {
                        app: app.clone(),
                        procs: Vec::new(),
                    })
                    .procs
                    .push(p.clone());
            }
        }
        // sessões ativas sem nenhum processo visto também contam (tempo de sessão)
        for sid in ativas {
            if !perfis.contains_key(sid) {
                match self.db.perfil_por_sid(sid).await {
                    Ok(x) => {
                        perfis.insert(sid.clone(), x.map(|c| c.profile_id));
                    }
                    Err(e) => {
                        rel.erros.push(format!("perfil não consultado: {e}"));
                        continue;
                    }
                }
            }
            if let Some(perfil_id) = perfis[sid] {
                grupos.entry(perfil_id).or_insert_with(|| GrupoPerfil {
                    sid: sid.clone(),
                    ativa: true,
                    apps: BTreeMap::new(),
                });
            }
        }
        grupos
    }

    async fn aplicar_sessao(
        &mut self,
        rel: &mut Relatorio,
        ativos: &mut HashSet<ChaveAtiva>,
        agora: DateTime<Utc>,
        perfil: Uuid,
        sid: &str,
        decisao: Decisao,
    ) {
        match decisao {
            Decisao::BloquearSessao { motivo } => {
                self.agir_na_sessao(rel, agora, perfil, sid, motivo).await
            }
            Decisao::Avisar {
                restante_segundos,
                motivo,
            } => {
                self.avisar(rel, ativos, perfil, None, sid, restante_segundos, motivo)
                    .await
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn aplicar_app(
        &mut self,
        rel: &mut Relatorio,
        ativos: &mut HashSet<ChaveAtiva>,
        perfil: Uuid,
        app_id: Uuid,
        sid: &str,
        g: &GrupoApp,
        decisao: Decisao,
    ) {
        match decisao {
            Decisao::EncerrarApp { motivo } => self.encerrar(rel, perfil, g, motivo).await,
            Decisao::Avisar {
                restante_segundos,
                motivo,
            } => {
                let nome = Some((app_id, g.app.process_name.as_str()));
                self.avisar(rel, ativos, perfil, nome, sid, restante_segundos, motivo)
                    .await
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn avisar(
        &mut self,
        rel: &mut Relatorio,
        ativos: &mut HashSet<ChaveAtiva>,
        perfil: Uuid,
        app: Option<(Uuid, &str)>,
        sid: &str,
        restante: u64,
        motivo: Motivo,
    ) {
        let app_id = app.map(|(id, _)| id);
        ativos.insert((perfil, app_id, motivo));
        let marco = if restante <= AVISOS_SEGUNDOS[1] {
            AVISOS_SEGUNDOS[1]
        } else {
            AVISOS_SEGUNDOS[0]
        };
        if self.avisados.contains(&(perfil, app_id, motivo, marco)) {
            return;
        }
        let nome = app.map(|(_, n)| n.to_owned());
        let aviso = Aviso {
            sid: sid.to_owned(),
            app: nome.clone(),
            restante_segundos: restante,
            motivo,
        };
        match self.sistema.avisar(&aviso) {
            Ok(()) => {
                self.avisados.insert((perfil, app_id, motivo, marco));
                let det = format!(
                    "aviso de {marco}s: restam {restante}s ({}){}",
                    motivo.as_str(),
                    nome.as_deref()
                        .map(|n| format!(" app {n}"))
                        .unwrap_or_default()
                );
                self.evento(
                    rel,
                    Some(perfil),
                    EventType::WarningShown,
                    TransactionStatus::Success,
                    det,
                )
                .await;
                rel.acoes.push(Acao::Avisou {
                    sid: sid.to_owned(),
                    app: nome,
                    restante_segundos: restante,
                    motivo,
                });
            }
            Err(e) => {
                let msg = format!("aviso não exibido ({}): {e}", motivo.as_str());
                self.evento(
                    rel,
                    Some(perfil),
                    EventType::InternalError,
                    TransactionStatus::Failure,
                    msg.clone(),
                )
                .await;
                rel.erros.push(msg);
            }
        }
    }

    async fn encerrar(&mut self, rel: &mut Relatorio, perfil: Uuid, g: &GrupoApp, motivo: Motivo) {
        for p in &g.procs {
            if self.abandonados.contains(&p.pid) {
                continue;
            }
            if let Some(n) = self.aguardando.get_mut(&p.pid) {
                *n += 1;
                if *n <= TICKS_DE_GRACA_APOS_ENCERRAR {
                    continue;
                }
                self.aguardando.remove(&p.pid);
            }
            match self.sistema.encerrar(p.pid) {
                Ok(()) => {
                    self.tentativas.remove(&p.pid);
                    self.aguardando.insert(p.pid, 0);
                    let det = format!(
                        "app {} (pid {}) encerrado: {}",
                        p.process_name,
                        p.pid,
                        motivo.as_str()
                    );
                    self.evento(
                        rel,
                        Some(perfil),
                        EventType::AppTerminated,
                        TransactionStatus::Success,
                        det,
                    )
                    .await;
                    rel.acoes.push(Acao::Encerrou {
                        pid: p.pid,
                        app: p.process_name.clone(),
                        motivo,
                    });
                }
                Err(e) => {
                    let n = self.tentativas.entry(p.pid).or_insert(0);
                    *n += 1;
                    let n = *n;
                    let msg = format!(
                        "falha ao encerrar {} (pid {}), tentativa {n}: {e}",
                        p.process_name, p.pid
                    );
                    self.evento(
                        rel,
                        Some(perfil),
                        EventType::InternalError,
                        TransactionStatus::Failure,
                        msg.clone(),
                    )
                    .await;
                    rel.erros.push(msg);
                    if n >= MAX_TENTATIVAS_ENCERRAR {
                        self.abandonados.insert(p.pid);
                        let det = format!(
                            "processo {} (pid {}) não encerra após {n} tentativas",
                            p.process_name, p.pid
                        );
                        self.evento(
                            rel,
                            Some(perfil),
                            EventType::TamperSuspected,
                            TransactionStatus::Failure,
                            det,
                        )
                        .await;
                    }
                }
            }
        }
    }

    /// Primeiro bloqueia a sessão; se a criança a desbloquear `MAX_BLOQUEIOS_ANTES_DO_LOGOFF`
    /// vezes com a restrição ainda valendo, encerra a sessão (logoff).
    async fn agir_na_sessao(
        &mut self,
        rel: &mut Relatorio,
        agora: DateTime<Utc>,
        perfil: Uuid,
        sid: &str,
        motivo: Motivo,
    ) {
        if let Some(t) = self.ultimo_bloqueio.get(sid)
            && (agora - *t).num_seconds() < REBLOQUEIO_SEGUNDOS
        {
            return;
        }
        let feitos = self.bloqueios.get(sid).copied().unwrap_or(0);
        if feitos >= MAX_BLOQUEIOS_ANTES_DO_LOGOFF {
            match self.sistema.encerrar_sessao(sid) {
                Ok(()) => {
                    self.ultimo_bloqueio.insert(sid.to_owned(), agora);
                    let det = format!(
                        "sessão encerrada (logoff) após {feitos} bloqueios desfeitos: {}",
                        motivo.as_str()
                    );
                    self.evento(
                        rel,
                        Some(perfil),
                        EventType::SessionLocked,
                        TransactionStatus::Success,
                        det,
                    )
                    .await;
                    rel.acoes.push(Acao::EncerrouSessao {
                        sid: sid.to_owned(),
                        motivo,
                    });
                }
                Err(e) => {
                    let msg = format!("sessão não encerrada ({}): {e}", motivo.as_str());
                    self.evento(
                        rel,
                        Some(perfil),
                        EventType::InternalError,
                        TransactionStatus::Failure,
                        msg.clone(),
                    )
                    .await;
                    rel.erros.push(msg);
                }
            }
            return;
        }
        match self.sistema.bloquear_sessao(sid) {
            Ok(()) => {
                self.ultimo_bloqueio.insert(sid.to_owned(), agora);
                self.bloqueios.insert(sid.to_owned(), feitos + 1);
                let det = format!("sessão bloqueada: {}", motivo.as_str());
                self.evento(
                    rel,
                    Some(perfil),
                    EventType::SessionLocked,
                    TransactionStatus::Success,
                    det,
                )
                .await;
                rel.acoes.push(Acao::Bloqueou {
                    sid: sid.to_owned(),
                    motivo,
                });
            }
            Err(e) => {
                let msg = format!("sessão não bloqueada ({}): {e}", motivo.as_str());
                self.evento(
                    rel,
                    Some(perfil),
                    EventType::InternalError,
                    TransactionStatus::Failure,
                    msg.clone(),
                )
                .await;
                rel.erros.push(msg);
            }
        }
    }
}
