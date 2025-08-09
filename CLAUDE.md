# Project-specific instructions for Claude

## Ansible Role Structure

When creating new roles, follow this structure:

1. Don't create galaxy_info or meta files for roles. Use `meta/main.yml` only to specify dependencies between roles, for example:
    ```
    dependencies:
      - { role: pkgmanager }
      - { role: user }
    ```

2. **Task organization** - Structure tasks as follows:
   - `tasks/main.yml` should ONLY contain includes:
     ```yaml
     ---
     - include: packages.yml
     - include: configs.yml
     - include: services.yml
     ```
   - `tasks/packages.yml` - for package installation tasks
   - `tasks/configs.yml` - for configuration file tasks
   - `tasks/services.yml` - for service management tasks
   - Do not create empty files, if there is nothing to add there

3. **Example structure for a role:**
   ```
   roles/rolename/
   ├── tasks/
   │   ├── main.yml      (only includes)
   │   ├── packages.yml
   │   ├── configs.yml
   │   └── services.yml
   ├── meta/
   │   └── main.yml
   ├── handlers/
   │   └── main.yml
   └── templates/
       └── (template files as needed)
   ```

## Other Conventions

- When creating udev rules, follow the pattern from existing roles (e.g., yubico role)
- Use handlers for reloading services or triggering system updates
- Use correct yaml. For example, instead of `become: yes` use `become: true`
- When naming tasks use very short description with the task type prefix. For example when you want to create `/etc/nginx/nginx.conf` name the task `config nginx`
- ansible.builtin.include has been removed. Use include_tasks or import_tasks instead.
- Use "apt" to install packages. Don't use "package", because these recipies will only be used on Ubuntu
