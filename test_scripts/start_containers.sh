#!/bin/bash
# Some of asuran's unit/integration tests require the ability to connect to specific services. If
# you don't want to stand up these services yourself, this script will launch the required docker
# containers. See stop_containers.sh and run_tests.sh for scripts that will tear down the containers
# and run the tests.

# NOTE: This script always uses the same ID's, but as they are prefixed with 'asuran-test', it is
# unlikely they will clash with any id's in use on your system, but please be aware of this.


# Stand up the container for testing SFTP
docker run --name "asuran_test_sftp" -d atmoz/sftp asuran:asuran

