#!/bin/bash
# This script sets up the environment used by asuran's testing harness
# It assumes the containers have already been created by start_containers

# Provides the IP of a container, given its id
container_ip () {
    docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' $1
}

# Setup environment variables for the sftp container
export ASURAN_SFTP_HOSTNAME="localhost"
export ASURAN_SFTP_PORT="2222"
export ASURAN_SFTP_USER="asuran"
export ASURAN_SFTP_PASS="asuran"
