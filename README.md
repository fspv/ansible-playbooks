Clone
=====

```
git clone https://github.com/prius/ansible-playbooks.git
git submodule update --init --recursive
```

Examples
========

Configure desktop environment

```
ansible-playbook -c ansible.cfg --diff -i hosts common-desktop.yml
```

Configure pure environment

```
ansible-playbook -c ansible.cfg --diff -i hosts common.yml
```

Playbooks
=========

# Setup Yubikey

```bash
#!/bin/sh


mkdir -p ~/.config/Yubico
pamu2fcfg > ~/.config/Yubico/u2f_keys


U2F_KEYS=/etc/Yubico/u2f_keys

sudo mkdir -p /etc/Yubico
test -f ~/.config/Yubico/u2f_keys && sudo mv  ~/.config/Yubico/u2f_keys $U2F_KEYS


if grep -q pam_u2f.so /etc/pam.d/common-auth;
then
    true
else
    echo "auth    required            pam_u2f.so nouserok authfile=${U2F_KEYS} cue" | sudo tee -a /etc/pam.d/common-auth
fi
```

# Setup Yubikey for server

0. Preserve a stable root ssh session to prevent locking you out

1. Validate OTP https://demo.yubico.com/otp/verify

2. /etc/ssh/sshd_config

```
PermitRootLogin yes
KbdInteractiveAuthentication yes
UsePAM yes
AuthenticationMethods publickey,keyboard-interactive:pam
```

2. /etc/pam.d/sshd

```
auth sufficient pam_yubico.so id=1234 debug authfile=/etc/yubikey
```

3. `systemctl restart sshd`
