# Quadro de Trabalho — E-007 F1: Assistente `kioskd configurar`

## Meta
Entregar o assistente de primeira configuração em terminal, conforme ADR-007 e ADR-009.
Os pais rodam **uma vez** na conta de administrador e terminam com o kiosk configurado, o serviço instalado e o código de pareamento na tela.

## Tarefas

### 1. Comando `kioskd configurar` (esqueleto)
- [ ] Registrar a forma do comando em `cli.rs` (nome, opções, modo interativo e `--respostas`)
- [ ] Adicionar ao roteador de comandos em `kioskd.rs`
- [ ] Fluxograma em `docs/architecture/fluxos-dos-comandos.md`
- [ ] Validar: `kioskd configurar --help` mostra o uso

### 2. Fontes da lista de programas (ADR-009)
- [ ] `sistema_windows::atalhos_do_menu` — ler `.lnk` do menu Iniciar (todos os utilizadores + perfil de cada criança)
- [ ] `sistema_windows::processos_em_execucao` — processos das sessões escolhidas
- [ ] Estrutura `ProgramaSugerido { nome_amigavel, executavel }`
- [ ] `kioskd configurar` mostra a lista (numerada, com nome e executável, marcando o que já está em `app_metadata`)
- [ ] Entrada manual como última opção

### 3. Fontes da lista de contas
- [ ] `sistema_windows::contas_locais` — contas padrão do Windows (exclui Administrador, convidado)
- [ ] `kioskd configurar` mostra a lista (numerada, com nome e SID, marcando o que já é perfil)
- [ ] Só as contas escolhidas viram `child_profile`

### 4. Fluxo interativo
- [ ] Boas-vindas e confirmação de que é administrador (senão recusa)
- [ ] Escolha das contas das crianças
- [ ] Escolha dos programas a controlar (com limite e janela por programa)
- [ ] Escolha do limite total por criança
- [ ] Configuração do endereço de escuta
- [ ] Resumo antes de gravar
- [ ] Execução: criar perfis, apps, regras, instalar serviço
- [ ] Código de pareamento no fim

### 5. Modo não interativo (`--respostas`)
- [ ] Formato do arquivo TOML
- [ ] Leitura e validação
- [ ] Mesmo fluxo sem interação
- [ ] Teste no CI (`tests/cli.rs` ou próprio `tests/configurar.rs`)

### 6. Fluxograma
- [ ] Diagrama Mermaid do comando `kioskd configurar`
- [ ] Validar com `mermaid-cli`
- [ ] O teste `tests/documentacao.rs` reconhece o comando novo

### 7. Testes
- [ ] Teste de ponta a ponta com `--respostas` no Linux (sem dependência Windows)
- [ ] Teste de recusa sem administrador no CI Windows
- [ ] Teste de idempotência: reexecutar não duplica

## Bloqueios
- A lista de contas e atalhos (tarefas 2 e 3) é Windows-only; no Linux/macOS, usar listas vazias ou mock para testes.
- `kioskd configurar` chama `kioskd instalar` no fim (já existe em `servico_windows.rs`).

## Feito
- (nada ainda)

## Fluxograma (esboço)
`kioskd configurar` (interativo ou `--respostas arquivo.toml`) → valida administrador → coleta dados → resume → grava (perfis, apps, regras) → instala serviço → mostra código de pareamento.