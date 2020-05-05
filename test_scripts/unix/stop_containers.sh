#!/bin/bash
# This script will remove any containers that are created by the start_containers script

# WARNING: Please keep in mind that this will stop and delete any containers that have the names
# defined in start_containers.sh. This script will force remove these containers.

# Destroy the container used for testing SFTP
docker stop "asuran_test_sftp"
docker rm "asuran_test_sftp" --force
