# Builds the Linux and Windows binaries in Docker and writes them to .\dist
# (docker writes its progress to stderr, so rely on the exit code only)
Set-Location $PSScriptRoot
$env:DOCKER_BUILDKIT = '1'
docker build -f docker/Dockerfile --target export --output type=local,dest=dist .
if ($LASTEXITCODE -ne 0) { Write-Error "docker build failed ($LASTEXITCODE)"; exit $LASTEXITCODE }
Get-ChildItem dist
