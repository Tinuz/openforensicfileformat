import ast
import pathlib
import unittest


REQUIRED_API = {
    "open_container",
    "read_manifest",
    "verify_container",
    "read_chunk",
    "verify_chunk",
    "map_offset_to_chunk",
    "read_file_index",
    "write_analysis_result",
    "append_provenance_event",
}


class TestSdkApiContract(unittest.TestCase):
    def setUp(self) -> None:
        root = pathlib.Path(__file__).resolve().parents[1]
        self.api_path = root / "offf_sdk" / "api.py"
        self.init_path = root / "offf_sdk" / "__init__.py"

    def _module_ast(self, path: pathlib.Path) -> ast.Module:
        return ast.parse(path.read_text(encoding="utf-8"))

    def test_api_module_defines_required_functions(self) -> None:
        tree = self._module_ast(self.api_path)
        defined = {
            node.name
            for node in tree.body
            if isinstance(node, ast.FunctionDef)
        }
        missing = sorted(REQUIRED_API.difference(defined))
        self.assertEqual(missing, [], f"missing function definitions in api.py: {missing}")

    def test_init_exports_required_functions(self) -> None:
        tree = self._module_ast(self.init_path)
        exported = set()

        for node in tree.body:
            if isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id == "__all__":
                        if isinstance(node.value, ast.List):
                            for elt in node.value.elts:
                                if isinstance(elt, ast.Constant) and isinstance(elt.value, str):
                                    exported.add(elt.value)

        missing = sorted(REQUIRED_API.difference(exported))
        self.assertEqual(missing, [], f"missing exports in __all__: {missing}")


if __name__ == "__main__":
    unittest.main()
