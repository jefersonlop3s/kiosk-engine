# ADR-012 — Imutabilidade do log forense por hash encadeado

Status: Proposto. Data: 2026-10-30. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-002](../epics/E-002-log-forense.md) F2.

## Contexto

O `Security_Log` ([E-002](../epics/E-002-log-forense.md)) é a fonte primária da verdade
forense (arquitetura, Seção I.3). Hoje ele é append-only por convenção do código (só INSERT),
mas sem proteção contra adulteração: uma conta com acesso ao banco (SYSTEM ou Administrador)
pode executar UPDATE/DELETE diretamente no SQLite.

A [ADR-001](ADR-001-enforcement-ativo-e-visivel.md) exige rastreabilidade. A E-002 F2 prevê
uma proteção contra UPDATE/DELETE e, se aprovada por ADR, hash encadeado.

A criança, como usuária padrão, não tem permissão de escrita no banco (só SYSTEM e
Administradores — ADR-002). O ataque modelado é o de um **administrador malicioso ou
comprometido** que tenta apagar ou altering evidências no log.

O dono adiou a decisão em 2026-10-28 com a condição de retomar só com ADR.

## Decisão

Proposta:

1. **Hash encadeado**: cada evento do `security_log` carrega o SHA-256 do `log_id` + `prev_hash`
   do evento anterior, formando uma corrente. O primeiro evento tem `prev_hash` nulo.
   O último evento conhecido pode ser verificado contra o banco.

2. **Implementação por trigger SQLite**: um trigger `BEFORE INSERT` calcula o hash do registro
   anterior e o armazena no novo registro. Isso garante que a corrente seja formada mesmo que
   a inserção venha de fora do código Rust (ex.: `sqlite3` CLI como administrador).

3. **Proteção contra UPDATE/DELETE**: um trigger `BEFORE UPDATE` e `BEFORE DELETE` emite
   `RAISE(ABORT)` para qualquer modificação em `security_log`, exceto da política de retenção
   (remoção de eventos com mais de 90 dias, que usa DELETE com uma condição explícita).

4. **Política de retenção**: a manutenção programada (remoção de eventos com mais de 90 dias)
   usa `DELETE FROM security_log WHERE timestamp < ?` e quebra a corrente. Isto é aceitável
   porque a retenção remove blocos completos — não eventos individuais — e o hash encadeado
   só garante imutabilidade dentro do bloco que permanece. O serviço registra um evento
   `SYSTEM / SUCCESS` com o detalhe `"retention: removed N events older than 90 days"`
   antes da remoção e outro depois. Um administrador pode verificar o hash do último evento
   que ficou comparando-o com o hash conhecido anterior à retenção.

5. **Verificação**: o comando `kioskd log verify` (a propor) percorre a corrente e confere
   os hashes. Não faz parte deste ADR; fica para uma fase futura.

Alternativas rejeitadas:
- **Somente trigger de UPDATE/DELETE** sem hash: impede adulteração, mas não oferece
  rastreabilidade da corrente. Um atacante com acesso ao banco poderia apagar tudo e
  recriar eventos legítimos.
- **Hash no código Rust em vez de trigger**: o hash seria tão forte, mas um administrador
  que insere diretamente no SQLite (fora do código Rust) quebraria a corrente sem
  deixar rastro. O trigger cobre esse caso.
- **Tabela separada de hashes**: mais complexo, sem ganho sobre o hash na própria linha.

## Consequências

- Ganho: adulteração do log deixa rastro detectável (corrente quebrada); trigger cobre
  INSERT fora do código Rust.
- Custo: cada inserção no log executa uma consulta extra (SHA-256 do registro anterior);
  o log de um perfil num dia cheio de enforcement pode ter centenas de eventos (testar
  impacto).
- Custo: a política de retenção quebra a corrente; documentar e registrar a quebra no
  próprio log.
- Risco: trigger `BEFORE INSERT` que falha (ex.: erro de hash) impede a inserção e
  derruba a transação. Testar exaustivamente na migração.
- Reversível: remover a migração e os triggers restaura o estado anterior; os eventos
  existentes mantêm o `prev_hash` como dado inerte.

## Referências e evolução

- Consome: [E-002](../epics/E-002-log-forense.md) F2.
- Substitui: decisão adiada de 2026-10-28 sobre hash encadeado.
- Depende de: migração SQLite segura (erro no trigger não pode quebrar o serviço).