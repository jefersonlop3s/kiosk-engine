# ADR-006 — Assinatura de requisições e pareamento de dispositivos

Status: Aceito em 2026-10-28 pelo dono (Jeferson Lopes). Data: 2026-10-28. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-005](../epics/E-005-api-comando-e-pareamento.md) F2; [arquitetura](../architecture/README.md) Seção III (Camadas 1 e 2).

## Contexto

A Camada 1 da arquitetura exige *proof-of-possession*: o comando só vale se assinado pelo dispositivo legítimo. O esquema (algoritmo, formato, anti-replay) e o pareamento nunca foram definidos. O alvo do ataque é a própria criança, com acesso físico ao PC, à rede doméstica e, possivelmente, à senha de um dispositivo dos pais deixado desbloqueado (ver risco na [ADR-002](ADR-002-identidade-por-conta-windows.md)).

## Decisão

Proposta:

1. **Par de chaves Ed25519 por dispositivo.** A chave privada nunca sai do dispositivo (no celular, no cofre do sistema; na CLI dos pais, em arquivo com permissão restrita). O serviço guarda só a **chave pública** na tabela `paired_device`.
2. **Assinatura de cada requisição** sobre uma string canônica: método, caminho, `timestamp` (UTC, segundos), `nonce` (16 bytes aleatórios) e SHA-256 do corpo. Cabeçalhos: `X-Kiosk-Device`, `X-Kiosk-Timestamp`, `X-Kiosk-Nonce`, `X-Kiosk-Signature`.
3. **Anti-replay**: rejeita `timestamp` fora de ±60 s do relógio do serviço e `nonce` já visto naquela janela (tabela `seen_nonce`, limpa a cada minuto). Como o relógio é do sistema e a criança não o altera (ADR-002), o desvio é aceitável; mudança de relógio detectada gera `TAMPER_SUSPECTED`.
4. **Pareamento presencial**: na conta de administrador, o serviço gera um código de uso único (8 caracteres, validade de 5 min, guardado só como hash). O dispositivo envia a chave pública e o código; o serviço grava o dispositivo e registra o evento. Sem código válido, o pareamento falha com `AUTH_FAIL`.
5. **Autorização**: no MVP há um único papel, **admin**, atribuído ao dispositivo pareado. Revogar um dispositivo é um comando admin e é imediato.
6. Falhas de autenticação, autorização e validação respondem sempre com o mesmo corpo genérico e gravam o motivo exato no log ([E-002](../epics/E-002-log-forense.md)): `AUTH_FAIL`, `AUTHZ_FAIL`, `VALIDATION_FAIL`.

Alternativas rejeitadas:
- **Token de portador (*bearer*)**: quem o copia, repete. Sem *proof-of-possession* e sem anti-replay.
- **HMAC com segredo compartilhado**: mais simples, mas o segredo existe nos dois lados; ler o banco do serviço (SYSTEM) ou o dispositivo vaza a capacidade de assinar como qualquer um. Chave pública no serviço não permite forjar.
- **mTLS com PKI própria**: forte, mas pesado: CA, emissão, revogação e renovação de certificados para uma casa.
- **RSA ou ECDSA P-256**: viáveis; Ed25519 tem assinatura determinística, chaves e assinaturas curtas e menos armadilhas de implementação.

## Consequências

- Ganho: roubar o banco do serviço não permite forjar comandos; replay e comando adulterado em trânsito são recusados.
- Custo: gerenciar chaves por dispositivo; perder o celular exige revogar pela CLI/conta de admin no PC.
- Risco: um dispositivo pareado e desbloqueado nas mãos da criança comanda tudo (sem segundo fator no MVP). Registrar como limitação; segundo fator ou PIN do app podem vir em E-006.
- Risco: dependência do relógio do serviço para a janela anti-replay.
- Reversível: o esquema é versionado no cabeçalho; troca de algoritmo exige novo pareamento.
- Validação: testes negativos em E-005 F2: sem assinatura, assinatura inválida, corpo alterado, `timestamp` velho e `nonce` repetido geram `AUTH_FAIL`.

## Referências e evolução

- Consome: [E-005](../epics/E-005-api-comando-e-pareamento.md) F2.
- Depende de: [ADR-005](ADR-005-rede-da-api-de-comando.md) (a assinatura vale em qualquer rede).
