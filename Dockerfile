# Use an official Rust runtime as a parent image
FROM ubuntu:22.04

# Set the working directory in the container
WORKDIR /l1x

# Install any necessary dependencies
RUN apt update && \
    apt install gnupg2 wget curl nano lsb-release --yes && \
    sh -c 'echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" > /etc/apt/sources.list.d/pgdg.list' && \
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor -o /etc/apt/trusted.gpg.d/postgresql.gpg && \
    apt update && \
    apt install libpq-dev git clang curl libssl-dev llvm libudev-dev make protobuf-compiler postgresql-client-16 --yes


# Execute the script when the container starts
CMD ["sleep","infinity"]