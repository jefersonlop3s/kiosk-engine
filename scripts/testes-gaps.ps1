<#
Testes dos GAPs abertos (GC-15, GC-16, GC-17, GC-20, GC-21) no PC Windows, sem kioskctl.
Roteiro: docs/runbooks/testes-dos-gaps.md

Rode num PowerShell ABERTO COMO ADMINISTRADOR, com a versao instalada (kioskd install).

  powershell -ExecutionPolicy Bypass -File .\testes-gaps.ps1   # GC-21, GC-16/17 e GC-15, tudo automatico (~10 min)
  .\testes-gaps.ps1 -Fase GC21      # so uma fase: GC21 | GC16 | GC15 | GC20 | Limpar
  .\testes-gaps.ps1 -Fase GC20 -Conta crianca   # GC-20 exige a conta da crianca (ela sera bloqueada)

IDs, SID e IP da LAN sao descobertos pelo script. A saida inteira vai para
testes-gaps-AAAAMMDD-HHMMSS.log ao lado do script: mande esse arquivo.
#>
param(
    [ValidateSet('Tudo', 'GC21', 'GC16', 'GC15', 'GC20', 'Limpar')]
    [string]$Fase = 'Tudo',
    [string]$Conta = $env:USERNAME,
    [int]$LimiteSegundos = 400
)

$ErrorActionPreference = 'Continue'
# o kioskd fala UTF-8; sem isto o PowerShell 5 mostra "Ã§" no lugar de "ç"
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }
$K = 'C:\Program Files\kiosk\kioskd.exe'
$LogServico = 'C:\ProgramData\kiosk\kioskd.log'
$Processo = 'notepad.exe'
$resultados = @()

$pasta = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
$arquivo = Join-Path $pasta ("testes-gaps-{0}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
Start-Transcript -Path $arquivo | Out-Null

function Titulo($t) { Write-Host "`n=== $t ===" -ForegroundColor Cyan }

function Resultado($nome, $ok, $detalhe) {
    $rotulo = if ($ok -eq $true) { 'PASSOU' } elseif ($ok -eq $false) { 'FALHOU' } else { 'VER' }
    $cor = if ($ok -eq $true) { 'Green' } elseif ($ok -eq $false) { 'Red' } else { 'Yellow' }
    Write-Host ("[{0}] {1} - {2}" -f $rotulo, $nome, $detalhe) -ForegroundColor $cor
    $script:resultados += [pscustomobject]@{ Teste = $nome; Resultado = $rotulo; Detalhe = $detalhe }
}

# Roda o kioskd instalado e devolve a saida (stdout+stderr) como texto.
function Kiosk {
    # ForEach-Object converte o stderr em texto: sem isso o PowerShell 5 imprime NativeCommandError
    $saida = & $K @args 2>&1 | ForEach-Object { "$_" } | Out-String
    return $saida.Trim()
}

function Uuid($texto) {
    return [regex]::Match([string]$texto, '[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}').Value
}

function UuidDaLinhaCom($texto, $trecho) {
    foreach ($linha in ($texto -split "`r?`n")) {
        if ($linha -match [regex]::Escape($trecho)) {
            $u = Uuid $linha
            if ($u) { return $u }
        }
    }
    return ''
}

function LinhasDoLog { if (Test-Path $LogServico) { @(Get-Content $LogServico -Encoding UTF8) } else { @() } }

# Mostra o que o servico gravou desde a contagem `$desde`; devolve a nova contagem.
function NovasLinhas($desde) {
    $todas = LinhasDoLog
    if ($todas.Count -gt $desde) {
        $todas[$desde..($todas.Count - 1)] | ForEach-Object { Write-Host "  log> $_" -ForegroundColor DarkGray }
    }
    return $todas.Count
}

function SidDaConta($nome) {
    if ($nome -eq $env:USERNAME) { return [Security.Principal.WindowsIdentity]::GetCurrent().User.Value }
    $u = Get-LocalUser -Name $nome -ErrorAction SilentlyContinue
    if ($u) { return $u.SID.Value }
    return ''
}

function PerfilId {
    $sid = SidDaConta $Conta
    if (-not $sid) { throw "conta '$Conta' nao encontrada (Get-LocalUser)" }
    $id = UuidDaLinhaCom (Kiosk profile list) $sid
    if (-not $id) {
        Write-Host "  criando perfil para $Conta ($sid)"
        $saida = Kiosk profile create --sid $sid --name "teste-gaps-$Conta"
        Write-Host "  $saida"
        $id = Uuid $saida
    }
    if (-not $id) { throw "nao consegui obter o ID do perfil de $Conta (veja a saida acima)" }
    return $id
}

function AppId {
    $id = UuidDaLinhaCom (Kiosk app list) $Processo
    if (-not $id) {
        $saida = Kiosk app create --process $Processo
        Write-Host "  $saida"
        $id = Uuid $saida
    }
    if (-not $id) { throw "nao consegui obter o ID do app $Processo" }
    return $id
}

function RevogarRegrasDeTeste($perfil) {
    foreach ($linha in ((Kiosk rule list --profile $perfil) -split "`r?`n")) {
        if ($linha -match 'ACTIVE|PENDING|ativ|pend') {
            $r = Uuid $linha
            if ($r) { Write-Host "  revogando regra $r"; Kiosk rule revoke $r | Out-Null }
        }
    }
}

# --- Preparacao ---------------------------------------------------------------------------------
function Preparar {
    Titulo 'Preparacao'
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $admin) { throw 'abra o PowerShell como Administrador' }
    if (-not (Test-Path $K)) { throw "nao achei $K (rode kioskd install)" }
    $status = Kiosk status
    Write-Host $status
    Write-Host (Kiosk version)
    $v = [regex]::Matches($status, 'vers\S+o[^:]*:\s*(\S+)') | ForEach-Object { $_.Groups[1].Value }
    Resultado 'versao instalada = versao da pasta' (($v | Select-Object -Unique).Count -eq 1) ($v -join ' / ')
    Resultado 'servico em execucao' ((Get-Service kiosk).Status -eq 'Running') (Get-Service kiosk).Status
}

# --- GC-21: recusa sem privilegio e com o servico parado -----------------------------------------
function ExecutarSemElevacao($argumentos) {
    $saida = Join-Path $env:TEMP 'kiosk-gc21.txt'
    Remove-Item $saida -ErrorAction SilentlyContinue
    $acao = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument "/c `"`"$K`" $argumentos > `"$saida`" 2>&1`""
    $princ = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName 'kiosk-gc21' -Action $acao -Principal $princ -Force | Out-Null
    try {
        Start-ScheduledTask -TaskName 'kiosk-gc21'
        for ($i = 0; $i -lt 30; $i++) {
            Start-Sleep -Seconds 1
            if ((Get-ScheduledTask -TaskName 'kiosk-gc21').State -ne 'Running' -and (Test-Path $saida)) { break }
        }
        if (Test-Path $saida) { return (Get-Content $saida -Raw -Encoding UTF8).Trim() }
        return '(sem saida)'
    } finally {
        Unregister-ScheduledTask -TaskName 'kiosk-gc21' -Confirm:$false -ErrorAction SilentlyContinue
    }
}

function TesteGC21 {
    Titulo 'GC-21: recusa sem elevacao, com o servico no ar e parado'
    $desde = (LinhasDoLog).Count
    $a = ExecutarSemElevacao 'pairing-code'
    Write-Host "  servico no ar, sem elevacao:`n  $a"
    Resultado 'GC-21 recusa sem elevacao' ($a -match 'administrador|administrator|elev') 'esperado: exige administrador elevado'
    $null = NovasLinhas $desde

    try {
        Stop-Service kiosk -ErrorAction Stop
        Start-Sleep -Seconds 2
        $b = ExecutarSemElevacao 'pairing-code'
        Write-Host "  servico parado, sem elevacao:`n  $b"
        Resultado 'GC-21 recusa com o servico parado diz que NAO foi registrada' `
        ($b -match '(?i)n.{1,3}o foi registrada|not (been )?(recorded|registered|logged)|no se (ha )?registrado') `
            'esperado: a mensagem informa que a tentativa nao foi registrada'
    } finally {
        Start-Service kiosk
        Start-Sleep -Seconds 3
        Resultado 'servico voltou' ((Get-Service kiosk).Status -eq 'Running') (Get-Service kiosk).Status
    }
    Write-Host '  (conferir que a 1a tentativa foi gravada exige "kioskctl log", bloqueado pelo SAC neste PC)'
}

# --- GC-16 e GC-17: agente como SYSTEM aplica regra; avisos pela bandeja --------------------------
function TesteGC16 {
    Titulo "GC-16/GC-17: regra de $LimiteSegundos s para $Processo na conta $Conta"
    $perfil = PerfilId
    $app = AppId
    Write-Host "  perfil=$perfil app=$app"
    RevogarRegrasDeTeste $perfil
    $saida = Kiosk rule create --profile $perfil --app $app --limit $LimiteSegundos --activate
    Write-Host "  $saida"
    $regra = Uuid $saida
    if (-not $regra) { Resultado 'GC-16 criar regra' $false $saida; return }

    $desde = (LinhasDoLog).Count
    Get-Process ($Processo -replace '\.exe$', '') -ErrorAction SilentlyContinue | Stop-Process -Force
    $inicio = Get-Date
    Start-Process $Processo
    Write-Host "  $Processo aberto. OLHE A BANDEJA: balao de 5 min (~$($LimiteSegundos - 300) s) e de 1 min (~$($LimiteSegundos - 60) s)." -ForegroundColor Yellow
    $encerrado = $false
    try {
        while (((Get-Date) - $inicio).TotalSeconds -lt ($LimiteSegundos + 180)) {
            Start-Sleep -Seconds 5
            $desde = NovasLinhas $desde
            if (-not (Get-Process ($Processo -replace '\.exe$', '') -ErrorAction SilentlyContinue)) { $encerrado = $true; break }
        }
    } finally {
        $seg = [int]((Get-Date) - $inicio).TotalSeconds
        Write-Host "  tempo ate o app fechar: $seg s"
        Kiosk rule revoke $regra | Out-Null
        Write-Host "  regra $regra revogada"
    }
    Resultado 'GC-16 agente encerrou o app no limite' ($encerrado -and $seg -ge ($LimiteSegundos - 60) -and $seg -le ($LimiteSegundos + 120)) "fechou em $seg s (esperado ~$LimiteSegundos s; se voce fechou na mao, vale como FALHOU)"
    $resp = Read-Host '  GC-17: voce viu o balao de aviso na bandeja (5 min e 1 min)? [s/n/parcial]'
    Resultado 'GC-17 avisos pela bandeja' $null "resposta do dono: $resp"
    $resp2 = Read-Host '  GC-17: a dica do icone mostrou o tempo restante nos ultimos minutos? [s/n]'
    Resultado 'GC-17/GC-22 dica com tempo restante' $null "resposta do dono: $resp2"
}

# --- GC-15: modo LAN ------------------------------------------------------------------------------
function TesteGC15 {
    Titulo 'GC-15: modo LAN (HTTP sem TLS)'
    $lan = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.IPAddress -match '^(192\.168\.|10\.|172\.(1[6-9]|2\d|3[01])\.)' } |
        Select-Object -First 1 -ExpandProperty IPAddress
    if (-not $lan) { Resultado 'GC-15 modo LAN' $null 'sem IP de LAN privado neste PC'; return }
    $listen = ([regex]::Match((Kiosk status), 'KIOSK_LISTEN=(\S+)')).Groups[1].Value
    if (-not $listen) { Resultado 'GC-15 modo LAN' $false 'nao achei KIOSK_LISTEN no status'; return }
    Write-Host "  LAN=$lan  listen atual=$listen"

    Remove-Item Env:\KIOSK_LAN -ErrorAction SilentlyContinue
    $sem = Kiosk install --listen "$listen,${lan}:8443"
    Write-Host "  sem KIOSK_LAN:`n  $sem"
    Resultado 'GC-15 sem KIOSK_LAN o endereco de LAN e recusado' ($sem -match 'recusado|refused|rejected') 'esperado: recusa'

    try {
        $env:KIOSK_LAN = '1'
        $com = Kiosk install --listen "$listen,${lan}:8443"
        Write-Host "  com KIOSK_LAN=1:`n  $com"
        Start-Sleep -Seconds 3
        $aceito = $com -notmatch 'recusado|refused|rejected'
        $alcanca = (Test-NetConnection -ComputerName $lan -Port 8443 -WarningAction SilentlyContinue).TcpTestSucceeded
        Write-Host (Kiosk status)
        Resultado 'GC-15 com KIOSK_LAN=1 o install aceita' $aceito 'esperado: aceita endereco privado'
        Resultado 'GC-15 a porta responde na LAN' $alcanca "${lan}:8443 (HTTP, sem TLS: GC-15 so fecha com TLS)"
    } finally {
        Remove-Item Env:\KIOSK_LAN -ErrorAction SilentlyContinue
        Write-Host "  restaurando o servico sem LAN:`n  $(Kiosk install --listen $listen)"
        Resultado 'servico restaurado' ((Get-Service kiosk).Status -eq 'Running') (Get-Service kiosk).Status
    }
}

# --- GC-20: logoff apos 2 desbloqueios (so conta da crianca) -------------------------------------
function TesteGC20 {
    Titulo "GC-20: logoff apos 2 desbloqueios na conta $Conta"
    if ($Conta -eq $env:USERNAME) {
        Resultado 'GC-20' $false 'passe -Conta <conta da crianca>: esta conta seria bloqueada e voce nao conseguiria limpar'
        return
    }
    $perfil = PerfilId
    Write-Host "  perfil=$perfil"
    Kiosk profile limit $perfil --seconds 1 | Out-Null
    $desde = (LinhasDoLog).Count
    Write-Host @"
  Limite total de 1 s aplicado: a conta $Conta bloqueia ao entrar.
  FACA AGORA (Win+L > trocar de usuario > $Conta):
    1. entre e espere o 1o bloqueio;
    2. espere 40 s e desbloqueie com a senha;  espere o 2o bloqueio;
    3. espere 40 s e desbloqueie de novo.      O esperado e o LOGOFF da conta.
  Este script acompanha o log por 15 min e remove o limite no fim.
"@ -ForegroundColor Yellow
    $inicio = Get-Date
    try {
        while (((Get-Date) - $inicio).TotalMinutes -lt 15) {
            Start-Sleep -Seconds 5
            $desde = NovasLinhas $desde
        }
    } finally {
        Kiosk profile limit $perfil --no-limit | Out-Null
        Write-Host '  limite removido (profile limit --no-limit)'
    }
    $r = Read-Host '  Houve logoff automatico apos o 2o desbloqueio? [s/n]'
    Resultado 'GC-20 logoff apos 2 desbloqueios' $null "resposta do dono: $r (o kioskd.log so traz inicio e parada; os eventos ficam no log forense, lido com kioskctl log)"
}

function Limpar {
    Titulo 'Limpeza'
    $perfil = PerfilId
    Kiosk profile limit $perfil --no-limit | Out-Null
    RevogarRegrasDeTeste $perfil
    Remove-Item Env:\KIOSK_LAN -ErrorAction SilentlyContinue
    if ((Get-Service kiosk).Status -ne 'Running') { Start-Service kiosk }
    Write-Host '  limite removido, regras revogadas, servico no ar'
}

# --- Execucao -------------------------------------------------------------------------------------
try {
    Write-Host "kioskd: $K  conta alvo: $Conta  fase: $Fase  ($(Get-Date -Format s))"
    Preparar
    switch ($Fase) {
        'Tudo'   { TesteGC21; TesteGC16; TesteGC15 }
        'GC21'   { TesteGC21 }
        'GC16'   { TesteGC16 }
        'GC15'   { TesteGC15 }
        'GC20'   { TesteGC20 }
        'Limpar' { Limpar }
    }
} catch {
    Resultado 'script' $false $_.Exception.Message
} finally {
    Titulo 'Resumo'
    $resultados | Format-Table -AutoSize -Wrap | Out-String | Write-Host
    Write-Host "Evidencia completa: $arquivo"
    Stop-Transcript | Out-Null
}
