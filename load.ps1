# Generates load on the test databases (docker/docker-compose.test.yml).
#   .\load.ps1                      # MySQL, 300 s, 4 OLTP workers
#   .\load.ps1 -Target both -Duration 600 -Workers 8
# Stop with Ctrl-C.
param(
    [ValidateSet('mysql', 'mariadb', 'both')]
    [string]$Target = 'mysql',
    [int]$Duration = 300,
    [int]$Workers = 4
)
Set-Location $PSScriptRoot
$hosts = if ($Target -eq 'both') { 'mysql,mariadb' } else { $Target }
docker compose -f docker/docker-compose.test.yml up -d mysql mariadb
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
docker compose -f docker/docker-compose.test.yml run --rm load $hosts $Duration $Workers
