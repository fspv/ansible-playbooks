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

