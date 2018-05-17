FROM ubuntu:xenial-20180123

# Install ansible build dependencies
RUN apt-get update && \
    apt-get -y install python-dev \
                       build-essential \
                       libssl-dev \
                       libffi-dev \
                       python-pip \
                       python-virtualenv \
                       git

# Install ansible
RUN mkdir -p /usr/lib/my/ && \
    cd /usr/lib/my/ && \
    virtualenv ansible && \
    . /usr/lib/my/ansible/bin/activate && \
    pip install ansible==2.4.0.0

ADD roles/ /etc/my/ansible/roles/
ADD hosts /etc/my/ansible/
ADD ansible.cfg /etc/my/ansible/
ADD docker-base-ubuntu.yml /etc/my/ansible/

# Apply ansible
RUN . /usr/lib/my/ansible/bin/activate && \
    cd /etc/my/ansible/ && \
    ansible-playbook --diff -i hosts docker-base-ubuntu.yml && \
    rm -rf /var/lib/apt/lists/ /var/cache/apt/
