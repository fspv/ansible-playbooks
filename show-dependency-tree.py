import glob
import yaml
from graphviz import Digraph

dot = Digraph()

for f in glob.glob('roles/*/meta/main.yml'):
    current_role = f.split('/')[1]
    with open(f) as dep_file:
        for dep in yaml.load(dep_file)['dependencies']:
            dot.edge(current_role, dep['role'])

dot.render('role-dependencies', view=True)
