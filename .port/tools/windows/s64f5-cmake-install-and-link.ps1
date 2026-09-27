# Parity S64-F5, Windows half (tasks W-1, W-1b): configure, build and install
# the CMake wrapper into a temporary prefix, show the installed layout, then
# link C consumers the way the installed .pc files and the CMake package
# config say to, run them, and check their output.
#
# Usage, on a Windows host with cmake, clang and cargo on PATH:
#   powershell -NoProfile -ExecutionPolicy Bypass -File s64f5-cmake-install-and-link.ps1 -Src <checkout> -Work <scratch dir>
# The scratch dir is deleted and recreated; delete it afterwards.
param(
    [Parameter(Mandatory = $true)][string]$Src,
    [Parameter(Mandatory = $true)][string]$Work
)
$ErrorActionPreference = "Stop"
if (Test-Path $Work) { Remove-Item -Recurse -Force $Work }
New-Item -ItemType Directory -Force $Work | Out-Null
$build = Join-Path $Work "build"
$prefix = Join-Path $Work "prefix"

function Run($exe, [string[]]$argv) {
    Write-Output ("> " + $exe + " " + ($argv -join " "))
    & $exe @argv 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE -ne 0) { throw "$exe exited $LASTEXITCODE" }
}

# Run a consumer and require example/c-vt's expected output, not only exit 0.
function RunChecked($exe) {
    $out = & $exe 2>&1 | Out-String
    Write-Output $out
    if ($LASTEXITCODE -ne 0) { throw "$exe exited $LASTEXITCODE" }
    if ($out -notmatch "Extracted title: hello") { throw "$exe did not print 'Extracted title: hello'" }
}

Run cmake @("-S", $Src, "-B", $build, "-DCMAKE_BUILD_TYPE=Release")
Run cmake @("--build", $build, "--config", "Release")
Run cmake @("--install", $build, "--prefix", $prefix, "--config", "Release")

Write-Output "== installed files"
Get-ChildItem -Recurse -File $prefix | Where-Object { $_.FullName -notlike "*\include\*" } |
    ForEach-Object { $_.FullName.Substring($prefix.Length + 1) + "  " + $_.Length }
Write-Output ("include headers: " + (Get-ChildItem -Recurse -File (Join-Path $prefix "include")).Count)

$pcdir = Join-Path $prefix "share\pkgconfig"
foreach ($pc in @("librustty-vt.pc", "librustty-vt-static.pc")) {
    Write-Output "== $pc"
    Get-Content (Join-Path $pcdir $pc) | Where-Object { $_ -match "^(libdir|Libs|Cflags)" }
}

# The .pc's Libs line, with ${libdir}/${includedir} expanded, is what a
# pkg-config consumer passes to the linker.
function PcField($pc, $key) {
    $line = Get-Content (Join-Path $pcdir $pc) | Where-Object { $_ -like "$($key):*" } | Select-Object -First 1
    $v = $line.Substring($key.Length + 1).Trim()
    $v = $v.Replace('${libdir}', (Join-Path $prefix "lib")).Replace('${includedir}', (Join-Path $prefix "include"))
    return $v
}

$main = Join-Path $Src "example\c-vt\src\main.c"

# Shared: the .pc's `-L${libdir} -lrustty-vt`, which must find the installed
# import library `rustty-vt.lib`; the DLL is installed to bin/.
$shared_exe = Join-Path $Work "shared.exe"
$libs = (PcField "librustty-vt.pc" "Libs") -split " "
Write-Output "== shared link: clang main.c -I<include> $($libs -join ' ')"
Run clang (@($main, "-I", (Join-Path $prefix "include"), "-o", $shared_exe) + $libs)
Copy-Item (Join-Path $prefix "bin\*.dll") $Work
Write-Output "== run shared.exe"
RunChecked $shared_exe

# Static: the static .pc's Libs + Libs.private, with RUSTTY_STATIC.
$static_exe = Join-Path $Work "static.exe"
$slibs = ((PcField "librustty-vt-static.pc" "Libs") -split " ") + ((PcField "librustty-vt-static.pc" "Libs.private") -split " ")
Write-Output "== static link: clang -DRUSTTY_STATIC main.c $($slibs -join ' ')"
Run clang (@("-DRUSTTY_STATIC", $main, "-I", (Join-Path $prefix "include"), "-o", $static_exe) + $slibs)
Write-Output "== run static.exe"
RunChecked $static_exe

# find_package(rustty-vt): the installed package config's IMPORTED_IMPLIB
# must name the installed import library.
$cons = Join-Path $Work "consumer"
New-Item -ItemType Directory -Force $cons | Out-Null
Copy-Item $main (Join-Path $cons "main.c")
Set-Content (Join-Path $cons "CMakeLists.txt") @"
cmake_minimum_required(VERSION 3.19)
project(w1consumer LANGUAGES C)
find_package(rustty-vt REQUIRED)
add_executable(consumer main.c)
target_link_libraries(consumer PRIVATE rustty-vt::rustty-vt)
"@
$cb = Join-Path $cons "build"
Run cmake @("-S", $cons, "-B", $cb, "-DCMAKE_PREFIX_PATH=$prefix")
Run cmake @("--build", $cb, "--config", "Release")
$cexe = Get-ChildItem -Recurse -Filter consumer.exe $cb | Select-Object -First 1
Copy-Item (Join-Path $prefix "bin\*.dll") $cexe.DirectoryName
Write-Output "== run consumer.exe (find_package)"
RunChecked $cexe.FullName
Write-Output "S64F5 OK"
