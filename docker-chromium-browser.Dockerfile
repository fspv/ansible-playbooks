FROM prius/base-ubuntu

ADD docker-chromium-browser.yml /etc/my/ansible/

# Run ansible
RUN . /usr/lib/my/ansible/bin/activate && \
    cd /etc/my/ansible/ && \
    ansible-playbook --diff -c ansible.cfg -i hosts --skip-tags skip_docker \
                     docker-chromium-browser.yml

RUN rm -rf /var/lib/apt/lists/ /var/cache/apt/
