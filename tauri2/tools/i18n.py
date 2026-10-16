#!/usr/bin/env python3
"""Keeps the eight translation catalogs in step. The JSON files in
ui/src/i18n/locales/<id>/common.json are the source; en-US sets the keys,
their order and the {placeholders}.

  tools/i18n.py check              every catalog has en-US's keys and placeholders
  tools/i18n.py unused             keys nothing in ui/src refers to
  tools/i18n.py add FILE [--after KEY]
                                   add or replace keys from FILE, a JSON object of
                                   key -> [en, ja, es, de, fr, it, pt, ru] (or a
                                   {locale: text} object); new keys go after KEY,
                                   or at the end
  tools/i18n.py remove KEY...      delete keys from every catalog

Counted phrases are named count.<thing> and use four keys in every
language, .one/.few/.many/.other; see plural() in ui/src/i18n/catalogs.ts.
"""
import glob, json, os, re, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ui", "src")
LOCALES = ["en-US", "ja-JP", "es-ES", "de-DE", "fr-FR", "it-IT", "pt-PT", "ru-RU"]


def path(locale):
    return os.path.join(ROOT, "i18n", "locales", locale, "common.json")


def load(locale):
    with open(path(locale), encoding="utf-8") as handle:
        return json.load(handle)


def save(locale, catalog):
    with open(path(locale), "w", encoding="utf-8") as handle:
        json.dump(catalog, handle, ensure_ascii=False, indent=2)
        handle.write("\n")


def placeholders(text):
    return sorted(set(re.findall(r"\{(\w+)\}", text)))


def check():
    english = load("en-US")
    problems = []
    for locale in LOCALES[1:]:
        catalog = load(locale)
        missing, extra = set(english) - set(catalog), set(catalog) - set(english)
        if missing or extra:
            problems.append(f"{locale}: missing {sorted(missing)} extra {sorted(extra)}")
        if list(catalog) != [key for key in english if key in catalog]:
            problems.append(f"{locale}: keys are not in en-US order")
        for key, text in english.items():
            if key in catalog and placeholders(catalog[key]) != placeholders(text):
                problems.append(f"{locale}: {key} has {placeholders(catalog[key])}, en-US has {placeholders(text)}")
    for key in english:
        base, _, form = key.rpartition(".")
        if base.startswith("count.") and form in ("one", "few", "many", "other"):
            lacking = [other for other in ("one", "few", "many", "other") if f"{base}.{other}" not in english]
            if lacking:
                problems.append(f"{base}: plural forms {lacking} are missing")
    print("\n".join(problems) if problems else f"{len(LOCALES)} catalogs, {len(english)} keys each")
    return 1 if problems else 0


def unused():
    source = ""
    for name in glob.glob(os.path.join(ROOT, "**", "*.ts*"), recursive=True):
        if ".test." not in name:
            with open(name, encoding="utf-8") as handle:
                source += handle.read()
    # t(`prefix.${value}`) and count("base", n) reach keys without naming them.
    prefixes = set(re.findall(r"`([\w.]+)\.\$\{", source)) | set(re.findall(r'count\("([\w.]+)"', source))
    keys = [key for key in load("en-US") if f'"{key}"' not in source and not any(key.startswith(prefix + ".") for prefix in prefixes)]
    print("\n".join(keys) if keys else "every key is used")
    return 0


def add(arguments):
    after = None
    if "--after" in arguments:
        position = arguments.index("--after")
        after = arguments[position + 1]
        del arguments[position : position + 2]
    with open(arguments[0], encoding="utf-8") as handle:
        entries = json.load(handle)
    for index, locale in enumerate(LOCALES):
        catalog = load(locale)
        fresh = {}
        for key, texts in entries.items():
            text = texts[locale] if isinstance(texts, dict) else texts[index]
            if key in catalog:
                catalog[key] = text
            else:
                fresh[key] = text
        if after is None:
            catalog.update(fresh)
        else:
            if after not in catalog:
                sys.exit(f"{after} is not a key")
            rebuilt = {}
            for key, text in catalog.items():
                rebuilt[key] = text
                if key == after:
                    rebuilt.update(fresh)
            catalog = rebuilt
        save(locale, catalog)
    return check()


def remove(keys):
    for locale in LOCALES:
        catalog = load(locale)
        for key in keys:
            catalog.pop(key, None)
        save(locale, catalog)
    return check()


def main():
    command, *arguments = sys.argv[1:] or ["check"]
    if command == "check":
        return check()
    if command == "unused":
        return unused()
    if command == "add" and arguments:
        return add(arguments)
    if command == "remove" and arguments:
        return remove(arguments)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
