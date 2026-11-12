import pathlib
import re

source = pathlib.Path(__file__).resolve().parents[1] / "src"
layers = {"domain", "application", "infrastructure", "presentation"}
errors = []
for module in sorted(path for path in source.iterdir() if path.is_dir()):
    for layer in layers:
        if not (module / layer / "mod.rs").is_file():
            errors.append(f"{module.name}: missing {layer} layer")
    for layer in ["domain", "application"]:
        for file in (module / layer).rglob("*.rs"):
            runtime = file.read_text().split("#[cfg(test)]")[0]
            forbidden = ["::infrastructure::", "::presentation::", "std::fs", "reqwest::", "ratatui::", "crossterm::", "rodio::", "dirs::"]
            for dependency in forbidden:
                if dependency in runtime:
                    errors.append(f"{file.relative_to(source)}: forbidden dependency {dependency}")
    if module.name != "shell":
        for file in (module / "infrastructure").rglob("*.rs"):
            runtime = file.read_text().split("#[cfg(test)]")[0]
            if re.search(r"crate::\w+::presentation::", runtime):
                errors.append(f"{file.relative_to(source)}: infrastructure depends on presentation")
for old_layer in layers:
    if (source / old_layer).exists():
        errors.append(f"global layer directory src/{old_layer} must be replaced by feature modules")
if errors:
    raise SystemExit("\n".join(errors))
print("Architecture passed: feature modules, four layers, independent domain and application services")
