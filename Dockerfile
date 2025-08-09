# Dockerfile
FROM ubuntu:24.04

# Install systemd and basic utilities
RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
    systemd \
    systemd-sysv \
    systemd-cron \
    sudo \
    curl \
    git \
    ansible \
    python3-pip \
    openssh-server \
    rsync \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

# Create ansible working directory
WORKDIR /ansible

# Copy ansible playbooks repository excluding manual directory
COPY . /ansible/

# Create manual configuration for container
RUN mkdir -p /ansible/manual
COPY docker-common.yml /ansible/manual/common.yml

# Run ansible using bootstrap script
RUN cd /ansible && \
    ./bootstrap.sh common-devserver.yml LOCAL

# Setup user environments with .bashrc
RUN for user in user admin; do \
        su - $user -c 'cd && \
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/fspv/.bashrc/refs/heads/master/.local/share/bin/bootstrap.sh)" && \
        .local/share/bin/init-user-env.sh' || true; \
    done

# Enable SSH service (optional)
RUN systemctl enable ssh

# Set systemd as entrypoint
VOLUME ["/sys/fs/cgroup", "/tmp", "/run", "/run/lock"]
CMD ["/lib/systemd/systemd"]
