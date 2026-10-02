"""Check architecture command construction; native installer tests exercise real lipo."""
import ast
from pathlib import Path
import shlex
import unittest

ROOT = Path(__file__).resolve().parents[2]


class MacArchitectureCommands(unittest.TestCase):
    def test_python_commands_put_the_input_before_verify_arch(self):
        for name in ('package.py', 'release_gate.py'):
            with self.subTest(script=name):
                source = ast.parse((ROOT / 'scripts' / name).read_text(encoding='utf-8'))
                commands = [node for node in ast.walk(source)
                            if isinstance(node, ast.List) and node.elts
                            and isinstance(node.elts[0], ast.Constant)
                            and node.elts[0].value == '/usr/bin/lipo']
                self.assertEqual(len(commands), 1)
                arguments = commands[0].elts
                self.assertEqual(len(arguments), 4)
                self.assertNotIsInstance(arguments[1], ast.Constant)
                self.assertEqual(ast.literal_eval(arguments[2]), '-verify_arch')
                self.assertIsInstance(arguments[3], ast.Name)
                self.assertEqual(arguments[3].id, 'arch')

    def test_shell_command_keeps_the_quoted_bundle_path_before_architectures(self):
        text = (ROOT / 'scripts/install_macos.sh').read_text(encoding='utf-8')
        commands = [shlex.split(line.strip()) for line in text.splitlines()
                    if line.strip().startswith('/usr/bin/lipo ')]
        self.assertEqual(len(commands), 1)
        self.assertEqual(commands[0][:4], [
            '/usr/bin/lipo', '$app/Contents/MacOS/maris', '-verify_arch', '$arch'])
        self.assertIn('||', commands[0])
        self.assertIn('fail', commands[0])


if __name__ == '__main__':
    unittest.main()
