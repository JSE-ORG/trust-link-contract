#!/usr/bin/env pwsh
# fix_create_escrow.ps1
#
# Rewrites every old-style create_escrow call in test files that passes a bare
# `&<seller_addr>` as the first argument, converting it to the new payees-Vec form.
#
# Old pattern (7 args):
#   client.create_escrow(
#       &<seller>,
#       &None::<Address>,
#       &resolver,
#       &token,
#       &amount,
#       &fee,
#       &window,
#   );
#
# New pattern (8 args):
#   let mut payees_N = Vec::new(&env);
#   payees_N.push_back(Payee { address: <seller>.clone(), bps: 10_000 });
#   client.create_escrow(
#       &payees_N,
#       &None::<Address>,
#       &resolver,
#       &token,
#       &amount,
#       &fee,
#       &0_u32,
#       &window,
#   );

$ErrorActionPreference = "Stop"
$srcDir = Join-Path $PSScriptRoot "contracts\escrow\src"
$testDir = Join-Path $PSScriptRoot "contracts\escrow\tests"

$files = @(
    Get-ChildItem -Path $srcDir  -Filter "test_*.rs" -File
    Get-ChildItem -Path $srcDir  -Filter "test.rs"   -File -ErrorAction SilentlyContinue
    Get-ChildItem -Path $testDir -Filter "*.rs"       -File
) | Where-Object { $null -ne $_ }

$counter = 0

foreach ($file in $files) {
    $raw = Get-Content -Raw $file.FullName
    $changed = $false

    # We look for the old 7-arg pattern. The regex captures:
    #  group 1: indentation before "let ... = client.create_escrow("  (or "client.create_escrow(")
    #  group 2: optional "let <id> = "
    #  group 3: the seller variable name (bare, without &)
    #  The remaining 6 args follow on their own lines.
    #
    # We match lazily so we only grab one call at a time.
    $pattern = '(?m)(?<indent>[ \t]*)(?<let>let\s+\w+\s*=\s*)?(?<client>\w+)\.create_escrow\(\r?\n(?<indent2>[ \t]*)&(?<seller>[A-Za-z_]\w*),\r?\n[ \t]*&None::<Address>,\r?\n[ \t]*&(?<resolver>[A-Za-z_][^,]+),\r?\n[ \t]*&(?<token>[A-Za-z_][^,]+),\r?\n[ \t]*&(?<amount>[A-Za-z_0-9_]+(?:_i128)?),\r?\n[ \t]*&(?<fee>[A-Za-z_0-9_]+(?:_u32)?),\r?\n[ \t]*&(?<window>[A-Za-z_0-9_]+(?:_u64)?),\r?\n[ \t]*\);'

    while ($raw -match $pattern) {
        $m = [regex]::Match($raw, $pattern)
        $counter++
        $pName = "payees_$counter"
        $indent = $m.Groups['indent'].Value
        $ind2   = $m.Groups['indent2'].Value
        $letPart = $m.Groups['let'].Value
        $clientPart = $m.Groups['client'].Value
        $seller  = $m.Groups['seller'].Value
        $resolver = $m.Groups['resolver'].Value
        $token    = $m.Groups['token'].Value
        $amount   = $m.Groups['amount'].Value
        $fee      = $m.Groups['fee'].Value
        $window   = $m.Groups['window'].Value

        $replacement = "${indent}let mut $pName = Vec::new(&env);" + [System.Environment]::NewLine +
                       "${indent}$pName.push_back(Payee { address: ${seller}.clone(), bps: 10_000 });" + [System.Environment]::NewLine +
                       "${indent}${letPart}${clientPart}.create_escrow(" + [System.Environment]::NewLine +
                       "${ind2}&$pName," + [System.Environment]::NewLine +
                       "${ind2}&None::<Address>," + [System.Environment]::NewLine +
                       "${ind2}&$resolver," + [System.Environment]::NewLine +
                       "${ind2}&$token," + [System.Environment]::NewLine +
                       "${ind2}&$amount," + [System.Environment]::NewLine +
                       "${ind2}&$fee," + [System.Environment]::NewLine +
                       "${ind2}&0_u32," + [System.Environment]::NewLine +
                       "${ind2}&$window," + [System.Environment]::NewLine +
                       "${indent});"

        $raw = $raw.Remove($m.Index, $m.Length).Insert($m.Index, $replacement)
        $changed = $true
    }

    if ($changed) {
        Set-Content -Path $file.FullName -Value $raw -NoNewline
        Write-Host "Patched: $($file.Name)"
    }
}

Write-Host "Done. Total replacements: $counter"
