#This script takes the arguments to `cargo test` as a single string, because windows
param(
    [String] $CargoArgs
)

# Some of asuran's unit/integration tests require the ability to connect to specific services. If
# you don't want to stand up these services yourself, this script will launch the required docker
# containers. See stop_containers.sh and run_tests.sh for scripts that will tear down the containers
# and run the tests.

# NOTE: This script always uses the same ID's, buct as they are prefixed with 'asuran-test', it is
# unlikely they will clash with any id's in use on your system, but please be aware of this.

# Stand up container for testing sftp
docker run --name "asuran_test_sftp" -p 2222:22 -d registry.gitlab.com/asuran-rs/sftp-docker:latest asuran:asuran:::asuran

# Utilize docker host for connection IPs, if set
$docker_ip = "localhost"
if (Test-Path env:DOCKER_HOST) {
    $docker_ip = ([System.Uri]"$env:DOCKER_HOST").Host
}

echo "Using $docker_ip for docker connections"

# Setup environment variables for the sftp container
$env:ASURAN_SFTP_HOSTNAME = $docker_ip
$env:ASURAN_SFTP_PORT = '2222'
$env:ASURAN_SFTP_USER = 'asuran'
$env:ASURAN_SFTP_PASS = 'asuran'

# Wait a few seconds to make sure the containers are all up
Start-Sleep -Seconds 3

# Run the tests
cargo test $CargoArgs
$RETURN_CODE = $LASTEXITCODE
# Some of asuran's unit/integration tests require the ability to connect to specific services. If
# you don't want to stand up these services yourself, this script will launch the required docker
# containers. See stop_containers.sh and run_tests.sh for scripts that will tear down the containers
# and run the tests.

# NOTE: This script always uses the same ID's, but as they are prefixed with 'asuran-test', it is
# unlikely they will clash with any id's in use on your system, but please be aware of this.
docker stop "asuran_test_sftp"
docker rm "asuran_test_sftp" --force

# Exit with the same code as `cargo test`
exit $RETURN_CODE