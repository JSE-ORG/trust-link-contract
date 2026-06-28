#!/usr/bin/env powershell
# fix_tests_safe.ps1
# Safe migration: rewrites old 7-arg create_escrow calls to the new 8-arg payees form.
# Also adds Payee/Vec imports where missing.
# Does NOT use PowerShell -replace with scriptblocks (which inject code as literals).

param()
$ErrorActionPreference = "Stop"

$srcDir  = Join-Path $PSScriptRoot "contracts\escrow\src"
$testDir = Join-Path $PSScriptRoot "contracts\escrow\tests"

$allFiles = @(
    Get-ChildItem -Path $srcDir  -Filter "test_*.rs" -File
    Get-ChildItem -Path $testDir -Filter "*.rs"       -File
) | Where-Object { $null -ne $_ }

# Regex: matches one multi-line create_escrow call with 7 args (old style).
# The first arg must be an address variable (starts with &<ident>).
# Captured groups:
#   ind   = leading indent of the let-binding or call
#   ind2  = indent of the argument list
#   let_  = optional "let <id> = " portion
#   cli   = client variable name
#   sel   = seller variable (without &)
#   buyer = second arg (could be &None::<Address> or &Some(...))
#   rest  = resolver, token, amount, fee, window line by line
$pattern = [regex]::new(
    '(?m)(?<ind>[ \t]*)(?<let_>let[ \t]+\w+[ \t]*=[ \t]*)?(?<cli>\w+)\.create_escrow\(\r?\n(?<ind2>[ \t]*)&(?<sel>[A-Za-z_]\w*),\r?\n(?<ind3>[ \t]*)(?<buyer>&(?:None|Some)[^,]+),\r?\n(?<ind4>[ \t]*)(?<resolver>&[^,]+),\r?\n(?<ind5>[ \t]*)(?<token>&[^,]+),\r?\n(?<ind6>[ \t]*)(?<amount>&[^,]+),\r?\n(?<ind7>[ \t]*)(?<fee>&[^,]+),\r?\n(?<ind8>[ \t]*)(?<window>&[^,]+),\r?\n(?<ind9>[ \t]*)\);',
    'Singleline'
)

$counter = 0

foreach ($file in $allFiles) {
    $raw = [System.IO.File]::ReadAllText($file.FullName)
    $changed = $false
    $needsPayeeImport = $false
    $needsVecImport   = $false

    # Iterate matches from end to start so indices stay valid after replacement
    $matches_ = $pattern.Matches($raw) | Sort-Object Index -Descending

    foreach ($m in $matches_) {
        $counter++
        $pName   = "payees_$counter"
        $ind     = $m.Groups['ind'].Value
        $ind2    = $m.Groups['ind2'].Value
        $let_    = $m.Groups['let_'].Value
        $cli     = $m.Groups['cli'].Value
        $sel     = $m.Groups['sel'].Value
        $buyer   = $m.Groups['buyer'].Value
        $resolver = $m.Groups['resolver'].Value
        $token   = $m.Groups['token'].Value
        $amount  = $m.Groups['amount'].Value
        $fee     = $m.Groups['fee'].Value
        $window  = $m.Groups['window'].Value
        $ind9    = $m.Groups['ind9'].Value

        $NL = "`r`n"
        $replacement  = "${ind}let mut $pName = Vec::new(&env);$NL"
        $replacement += "${ind}$pName.push_back(Payee { address: ${sel}.clone(), bps: 10_000 });$NL"
        $replacement += "${ind}${let_}${cli}.create_escrow($NL"
        $replacement += "${ind2}&$pName,$NL"
        $replacement += "${ind2}${buyer},$NL"
        $replacement += "${ind2}${resolver},$NL"
        $replacement += "${ind2}${token},$NL"
        $replacement += "${ind2}${amount},$NL"
        $replacement += "${ind2}${fee},$NL"
        $replacement += "${ind2}&0_u32,$NL"
        $replacement += "${ind2}${window},$NL"
        $replacement += "${ind9});"

        $raw = $raw.Remove($m.Index, $m.Length).Insert($m.Index, $replacement)
        $changed = $true
        $needsPayeeImport = $true
        $needsVecImport   = $true
    }

    if ($changed) {
        # --- Add Payee import if missing ---
        if ($needsPayeeImport -and ($raw -notmatch '\bPayee\b')) {
            # Try to append to existing use crate::{...} block
            if ($raw -match '(?m)^(use crate::\{)([^\}]+)(\})') {
                $old = $Matches[0]
                $new = $Matches[1] + $Matches[2].TrimEnd() + ', Payee' + $Matches[3]
                $raw = $raw.Replace($old, $new)
            }
        }

        # --- Add Vec import if missing ---
        if ($needsVecImport -and ($raw -notmatch 'soroban_sdk::[^;]*\bVec\b')) {
            if ($raw -match '(?m)^([ \t]*use soroban_sdk::\{)([^\}]+)(\})') {
                $old = $Matches[0]
                $new = $Matches[1] + $Matches[2].TrimEnd() + ', Vec' + $Matches[3]
                $raw = $raw.Replace($old, $new)
            }
        }

        [System.IO.File]::WriteAllText($file.FullName, $raw)
        Write-Host "Patched: $($file.Name)"
    }
}

Write-Host ""
Write-Host "Done. Total create_escrow calls migrated: $counter"
