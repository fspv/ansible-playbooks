from os import path
from typing import Dict, List

from ansible.module_utils._text import to_text
from ansible.plugins.action import ActionBase


class ActionModule(ActionBase):
    def run(self, tmp: str = None, task_vars: Dict = None) -> Dict:
        """ Load specified yml files and merge them.
        Lists on the leaf nodes of the dict tree will be merged as well.
        If types don't match error will be raised
        """
        result = {}

        result["ansible_facts"] = self._load_files_in_dir(
            path.join(self._task._role._role_path, "vars"),
            self._task.args.get("files", None),
        )

        return result

    def _load_file(self, filename: str) -> Dict:
        b_data, show_content = self._loader._get_file_contents(filename)
        data = to_text(b_data, errors="surrogate_or_strict")
        data = self._loader.load(data, file_name=filename, show_content=show_content)

        return data

    def _load_files_in_dir(self, root_dir: str, var_files: List[str]) -> Dict:
        results: Dict = {}
        for filename in var_files:
            filepath = path.join(root_dir, filename)
            if path.exists(filepath):
                data = self._load_file(filepath)
                self._merge(results, data)

        return results

    def _merge(self, left: Dict, right: Dict) -> None:
        for key in right.keys():
            left[key] = left.get(key, type(right[key])())

            if type(left[key]) != type(right[key]):
                raise TypeError(
                    f"Types of values should match: {left[key]} != {right[key]}"
                )

            if isinstance(left[key], list):
                left[key].extend(right[key])

            if isinstance(left[key], dict):
                self._merge(left[key], right[key])
