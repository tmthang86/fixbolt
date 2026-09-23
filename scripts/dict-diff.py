#!/usr/bin/env python3
"""Compare the QuickFIX FIX 4.4 dictionary with the FIX Orchestra FIX 4.4 file.

ADR-0101 decision 1: one program, standard library only, no change to build.rs
or any crate. It flattens both files into ONE model the way
crates/dict/build.rs flattens QuickFIX's XML (components expanded, groups
nested, header / trailer separated), compares nine dimensions, prints one row
per divergence keyed by content, classes each row gate-visible or quiet, and
prints the `OUTCOME:` line ADR-0101 decision 3's rule gives.

    python3 scripts/dict-diff.py                  # the real run
    python3 scripts/dict-diff.py --self-check     # QuickFIX vs itself
    python3 scripts/dict-diff.py --mutation-check # Orchestra vs a mutated copy

The program has to prove itself before it is believed (ADR-0101 decision 1):
QuickFIX against itself must give zero rows and reproduce the counts the Rust
tests already assert — 912 tags, 912 types, 93 message types, 12 524 body
pairs, 1 708 enum values, 30 header tags, 16 DATA fields, 731 group keys — and
a copy of the Orchestra file with two deliberate edits must give exactly two
rows, one in G and one in E. Those expected numbers are fixed here, from the
tests; if the program disagrees with them the program is wrong, not the data.

Inputs are fetched, never committed (CLAUDE.md §2 item 9, ADR-0101 decision 2):
scripts/fetch-quickfix-assets.sh and scripts/fetch-orchestra-assets.sh.
Outputs land under target/dict-diff/ (gitignored).
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import re
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QF_XML = os.path.join(REPO, "vendor/quickfix/spec/FIX44.xml")
ORCH_XML = os.path.join(REPO, "vendor/orchestra/OrchestraFIX44.xml")
DEF_DIR = os.path.join(REPO, "vendor/quickfix/test/definitions/server/fix44")
OUT_DIR = os.path.join(REPO, "target/dict-diff")

ORCH_PINNED_SHA256 = "a36262895e90bbcad2948e0c98072173a571a253ba63107a89617441c67a9f86"

# The QuickFIX-side counts the Rust tests assert. Source of each, so a reader
# can check they were copied and not tuned:
#   912    crates/dict/tests/common/mod.rs:129,143; interop_quickfix_fields.rs:69
#   93     crates/dict/tests/interop_quickfix_messages.rs (quickfix.len() == 93)
#   12 524 crates/dict/tests/interop_quickfix_messages.rs:50
#   1 708  crates/dict/tests/enums.rs:110
#   30     crates/dict/tests/interop_quickfix_messages.rs (header.len() == 30)
#   16     build.rs data_length_tag doc comment; DESIGN / CONFORMANCE "16 DATA"
#   731    crates/dict/tests/group_tables.rs:98; interop_quickfix_order.rs:232
EXPECTED_COUNTS = {
    "T": 912,
    "Y": 912,
    "M": 93,
    "P": 12524,
    "E": 1708,
    "H": 30,
    "L": 16,
    "G": 731,
}

# ADR-0101 decision 3: the seven FIX 4.4 session messages.
SESSION_MSG_TYPES = {"0", "1", "2", "3", "4", "5", "A"}
DEF_FILES_EXPECTED = 59

# Decision-rule thresholds, ADR-0101 decision 3. Fixed there before any
# measurement; copied, not chosen.
A_QUIET_MAX = 25
B_TOTAL_MAX = 150
B_OVERLAY_MAX = 25

DIMS = ["T", "Y", "M", "P", "R", "E", "H", "L", "G"]
ALWAYS_GATE_VISIBLE = {"M", "H", "L", "G"}

# crates/dict/src/field_type.rs `FieldType::from_xml`, copied verbatim as a
# mapping from the XML spelling to the variant. Orchestra spells datatypes in
# mixed case (`int`, `UTCTimestamp`, `data`); the one interpretation made here
# is that the Orchestra spelling upper-cased IS the QuickFIX spelling, which
# holds for all 25 datatypes the pinned file declares (checked below: an
# Orchestra type that does not map is reported, not guessed).
FROM_XML = {
    "INT": "Int",
    "LENGTH": "Length",
    "SEQNUM": "SeqNum",
    "NUMINGROUP": "NumInGroup",
    "FLOAT": "Float",
    "QTY": "Qty",
    "PRICE": "Price",
    "PRICEOFFSET": "PriceOffset",
    "AMT": "Amt",
    "PERCENTAGE": "Percentage",
    "CHAR": "Char",
    "BOOLEAN": "Boolean",
    "STRING": "String",
    "MULTIPLEVALUESTRING": "MultipleValueString",
    "CURRENCY": "Currency",
    "EXCHANGE": "Exchange",
    "COUNTRY": "Country",
    "MONTHYEAR": "MonthYear",
    "LOCALMKTDATE": "LocalMktDate",
    "UTCDATEONLY": "UtcDateOnly",
    "UTCTIMEONLY": "UtcTimeOnly",
    "UTCTIMESTAMP": "UtcTimestamp",
    "DATA": "Data",
    "XID": "String",
    "XIDREF": "String",
    "MULTIPLESTRINGVALUE": "MultipleValueString",
    "XMLDATA": "Data",
    "LANGUAGE": "Language",
    "TAGNUM": "TagNum",
    "MULTIPLECHARVALUE": "MultipleCharValue",
    "LOCALMKTTIME": "LocalMktTime",
    "TZTIMEONLY": "TzTimeOnly",
    "TZTIMESTAMP": "TzTimestamp",
}


class DictError(Exception):
    """A condition under which build.rs would refuse to generate (its `die`)."""


# ---------------------------------------------------------------------------
# The neutral model both readers fill.
# ---------------------------------------------------------------------------


@dataclass
class Model:
    source: str
    name_of: dict[int, str] = field(default_factory=dict)  # T
    type_of: dict[int, str] = field(default_factory=dict)  # Y (variant)
    raw_type_of: dict[int, str] = field(default_factory=dict)  # spelling, for rows
    enums: set[tuple[int, str]] = field(default_factory=set)  # E
    msg_name: dict[str, str] = field(default_factory=dict)  # M
    msg_admin: dict[str, bool] = field(default_factory=dict)  # informational
    body: dict[str, set[int]] = field(default_factory=dict)  # P
    required: dict[str, set[int]] = field(default_factory=dict)  # R
    header: set[int] = field(default_factory=set)  # H
    trailer: set[int] = field(default_factory=set)  # H
    header_required: set[int] = field(default_factory=set)  # informational
    data_len: dict[int, int | None] = field(default_factory=dict)  # L
    groups: dict[tuple[str, int], list[int]] = field(default_factory=dict)  # G
    group_positions: int = 0
    # Orchestra only: which group definition produced which (msg, counter)
    # keys, so --mutation-check can pick a group whose edit touches one key.
    group_def_keys: dict[str, set[tuple[str, int]]] = field(default_factory=dict)
    codeset_fields: dict[str, list[int]] = field(default_factory=dict)
    # Orchestra only: DATA pairing by the build.rs name rule, to report where
    # the explicit lengthId and the name rule would disagree.
    data_len_by_name: dict[int, int | None] = field(default_factory=dict)
    notes: list[str] = field(default_factory=list)

    def counts(self) -> dict[str, int]:
        return {
            "T": len(self.name_of),
            "Y": len(self.type_of),
            "M": len(self.msg_name),
            "P": sum(len(v) for v in self.body.values()),
            "E": len(self.enums),
            "H": len(self.header),
            "L": len(self.data_len),
            "G": len(self.groups),
        }


def sha256_of(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def pair_data_by_name(model: Model) -> dict[int, int | None]:
    """build.rs `emit`: DATA -> LENGTH, matched by NAME (`{name}Len`,
    `{name}Length`), never by tag - 1. FIX 4.4 has no length exceptions."""
    number_of = {n: t for t, n in model.name_of.items()}
    out: dict[int, int | None] = {}
    for tag, ty in model.type_of.items():
        if ty != "Data":
            continue
        name = model.name_of[tag]
        cand = None
        for c in (f"{name}Len", f"{name}Length"):
            if c in number_of:
                cand = number_of[c]
                break
        out[tag] = cand
    return out


# ---------------------------------------------------------------------------
# QuickFIX reader — a line-by-line port of crates/dict/build.rs `generate`.
# ---------------------------------------------------------------------------


def read_quickfix(path: str) -> Model:
    root = ET.parse(path).getroot()
    m = Model(source=f"QuickFIX {os.path.relpath(path, REPO)}")

    def child(el, name):
        for c in el:
            if c.tag == name:
                return c
        return None

    # collect_fields
    fields_el = child(root, "fields")
    if fields_el is None:
        raise DictError("<fields> section missing")
    number_of: dict[str, int] = {}
    for f in fields_el:
        if f.tag != "field":
            continue
        name, num = f.get("name"), f.get("number")
        if name is None or num is None:
            raise DictError("<field> without name or number")
        tag = int(num)
        if name in number_of:
            raise DictError(f"field name {name} appears twice")
        if tag in m.name_of:
            raise DictError(f"fields {m.name_of[tag]} and {name} both carry {tag}")
        number_of[name] = tag
        m.name_of[tag] = name
        ty = f.get("type", "")
        if ty not in FROM_XML:
            raise DictError(f"field {name} has type {ty!r}, unknown to FieldType")
        m.type_of[tag] = FROM_XML[ty]
        m.raw_type_of[tag] = ty
        for v in f:
            if v.tag == "value":
                e = v.get("enum")
                if e is None:
                    raise DictError(f"field {name} has a <value> with no enum")
                m.enums.add((tag, e))

    components = {}
    comps_el = child(root, "components")
    if comps_el is not None:
        for c in comps_el:
            if c.tag == "component" and c.get("name") is not None:
                components[c.get("name")] = c
    for name, c in components.items():
        if not any(True for _ in c):
            raise DictError(f"component {name} is empty")

    header_el, trailer_el = child(root, "header"), child(root, "trailer")
    messages_el = child(root, "messages")
    if header_el is None or trailer_el is None or messages_el is None:
        raise DictError("<header>, <trailer> or <messages> missing")

    def tag_of(name, ctx):
        if name not in number_of:
            raise DictError(f"{ctx} names unknown field {name}")
        return number_of[name]

    def comp(name, ctx, path):
        if name not in components:
            raise DictError(f"{ctx} names unknown component {name}")
        if name in path:
            raise DictError(f"component cycle: {' -> '.join(path)} -> {name}")
        return components[name]

    # collect_header — descends into <group>
    def collect_header(el, out):
        for c in el:
            if c.tag not in ("field", "group"):
                continue
            out.add(tag_of(c.get("name"), "header"))
            if c.tag == "group":
                collect_header(c, out)

    collect_header(header_el, m.header)
    collect_header(trailer_el, m.trailer)
    # required_header — direct children only, exactly as build.rs reads it.
    for c in header_el:
        if c.get("required") == "Y" and c.get("name") in number_of:
            m.header_required.add(number_of[c.get("name")])

    # collect_required
    def collect_required(el, msg, out, path):
        for c in el:
            if c.get("required") != "Y":
                continue
            name = c.get("name")
            if name is None:
                continue
            if c.tag in ("field", "group"):
                out.add(tag_of(name, f"message {msg}"))
            elif c.tag == "component":
                d = comp(name, f"message {msg}", path)
                collect_required(d, msg, out, path + [name])

    # collect_allowed
    def collect_allowed(el, msg, out, path):
        for c in el:
            name = c.get("name")
            if name is None:
                continue
            if c.tag in ("field", "group"):
                out.add(tag_of(name, f"message {msg}"))
                if c.tag == "group":
                    collect_allowed(c, msg, out, path)
            elif c.tag == "component":
                d = comp(name, f"message {msg}", path)
                collect_allowed(d, msg, out, path + [name])

    # collect_members
    def collect_members(el, ctx, out, path):
        for c in el:
            name = c.get("name")
            if name is None:
                continue
            if c.tag in ("field", "group"):
                out.append(tag_of(name, ctx))
            elif c.tag == "component":
                d = comp(name, ctx, path)
                collect_members(d, ctx, out, path + [name])

    # collect_groups
    def collect_groups(el, mt, path):
        for c in el:
            name = c.get("name")
            if name is None:
                continue
            if c.tag == "group":
                counter = tag_of(name, f"message {mt} group counter")
                members: list[int] = []
                collect_members(c, f"group {name}", members, [])
                if not members:
                    raise DictError(f"group {name} in message {mt} has no members")
                m.group_positions += 1
                key = (mt, counter)
                if key in m.groups and m.groups[key] != members:
                    raise DictError(f"counter {counter} twice in {mt}, different members")
                m.groups[key] = members
                collect_groups(c, mt, path)
            elif c.tag == "component":
                d = comp(name, f"message {mt}", path)
                collect_groups(d, mt, path + [name])

    for msg in messages_el:
        if msg.tag != "message":
            continue
        name, mt = msg.get("name"), msg.get("msgtype")
        if name is None or mt is None:
            raise DictError("<message> without name or msgtype")
        if mt in m.msg_name:
            raise DictError(f"two messages share msgtype {mt}")
        cat = msg.get("msgcat")
        if cat not in ("admin", "app"):
            raise DictError(f"message {name} ({mt}) has msgcat={cat!r}")
        m.msg_name[mt] = name
        m.msg_admin[mt] = cat == "admin"
        req: set[int] = set()
        collect_required(msg, name, req, [])
        m.required[mt] = req
        allowed: set[int] = set()
        collect_allowed(msg, name, allowed, [])
        m.body[mt] = allowed
    for msg in messages_el:
        if msg.tag == "message":
            collect_groups(msg, msg.get("msgtype"), [])
    collect_groups(header_el, "", [])

    m.data_len = pair_data_by_name(m)
    for d, ln in m.data_len.items():
        if ln is None:
            raise DictError(f"DATA field {m.name_of[d]} has no length field")
    return m


# ---------------------------------------------------------------------------
# Orchestra reader — the same flattening over Orchestra's shapes.
# ---------------------------------------------------------------------------

FIXR = "{http://fixprotocol.io/2020/orchestra/repository}"
HEADER_COMPONENT = "StandardHeader"
TRAILER_COMPONENT = "StandardTrailer"


def read_orchestra(path: str) -> Model:
    root = ET.parse(path).getroot()
    m = Model(source=f"Orchestra {os.path.relpath(path, REPO)}")

    def section(name):
        el = root.find(FIXR + name)
        if el is None:
            raise DictError(f"<fixr:{name}> section missing")
        return el

    def kids(el, *names):
        want = {FIXR + n for n in names}
        return [c for c in el if c.tag in want]

    datatypes = {d.get("name") for d in kids(section("datatypes"), "datatype")}
    codesets = {}
    for cs in kids(section("codeSets"), "codeSet"):
        if cs.get("name") in codesets:
            raise DictError(f"codeSet {cs.get('name')} appears twice")
        codesets[cs.get("name")] = cs

    for f in kids(section("fields"), "field"):
        tag, name, ty = int(f.get("id")), f.get("name"), f.get("type")
        if tag in m.name_of:
            raise DictError(f"field id {tag} appears twice")
        m.name_of[tag] = name
        if ty in codesets:
            cs = codesets[ty]
            m.codeset_fields.setdefault(ty, []).append(tag)
            base = cs.get("type")
            for code in kids(cs, "code"):
                m.enums.add((tag, code.get("value")))
        elif ty in datatypes:
            base = ty
        else:
            raise DictError(f"field {name}({tag}) has type {ty!r}: no datatype, no codeSet")
        m.raw_type_of[tag] = base if ty == base else f"{ty}->{base}"
        m.type_of[tag] = FROM_XML.get(base.upper(), f"UNMAPPED({base})")
        if f.get("lengthId") is not None:
            m.data_len[tag] = int(f.get("lengthId"))
    # A DATA field without lengthId is a divergence in L, not a crash: keep
    # it visible as `None`.
    for tag, ty in m.type_of.items():
        if ty == "Data" and tag not in m.data_len:
            m.data_len[tag] = None
    for tag in list(m.data_len):
        if m.type_of.get(tag) != "Data":
            m.notes.append(
                f"field {m.name_of[tag]}({tag}) carries lengthId but is typed "
                f"{m.type_of.get(tag)}; kept in L"
            )
    m.data_len_by_name = pair_data_by_name(m)

    components = {}
    comp_by_id = {}
    for c in kids(section("components"), "component"):
        components[c.get("name")] = c
        comp_by_id[c.get("id")] = c
    groups_by_id = {}
    for g in kids(section("groups"), "group"):
        groups_by_id[g.get("id")] = g

    def ref_tag(c, ctx):
        tag = int(c.get("id"))
        if tag not in m.name_of:
            raise DictError(f"{ctx} names unknown field id {tag}")
        return tag

    def comp(c, ctx, path):
        cid = c.get("id")
        if cid not in comp_by_id:
            raise DictError(f"{ctx} names unknown component id {cid}")
        d = comp_by_id[cid]
        if cid in path:
            raise DictError(f"component cycle through {d.get('name')}")
        if not kids(d, "fieldRef", "groupRef", "componentRef"):
            raise DictError(f"component {d.get('name')} is empty")
        return d

    def grp(c, ctx):
        gid = c.get("id")
        if gid not in groups_by_id:
            raise DictError(f"{ctx} names unknown group id {gid}")
        g = groups_by_id[gid]
        nig = kids(g, "numInGroup")
        if len(nig) != 1:
            raise DictError(f"group {g.get('name')} has {len(nig)} numInGroup")
        counter = int(nig[0].get("id"))
        if counter not in m.name_of:
            raise DictError(f"group {g.get('name')} counter {counter} unknown")
        return g, counter

    def is_envelope(c):
        if c.tag != FIXR + "componentRef":
            return False
        d = comp_by_id.get(c.get("id"))
        return d is not None and d.get("name") in (HEADER_COMPONENT, TRAILER_COMPONENT)

    members_of = ("fieldRef", "groupRef", "componentRef")

    # Header and trailer: every tag, descending into groups.
    def collect_header(el, out):
        for c in kids(el, *members_of):
            if c.tag == FIXR + "fieldRef":
                out.add(ref_tag(c, "header"))
            elif c.tag == FIXR + "groupRef":
                g, counter = grp(c, "header")
                out.add(counter)
                collect_header(g, out)
            else:
                collect_header(comp(c, "header", []), out)

    if HEADER_COMPONENT not in components or TRAILER_COMPONENT not in components:
        raise DictError("StandardHeader or StandardTrailer component missing")
    header_el = components[HEADER_COMPONENT]
    trailer_el = components[TRAILER_COMPONENT]
    collect_header(header_el, m.header)
    collect_header(trailer_el, m.trailer)
    for c in kids(header_el, "fieldRef", "groupRef"):
        if c.get("presence") == "required":
            m.header_required.add(grp(c, "header")[1] if c.tag == FIXR + "groupRef" else ref_tag(c, "header"))

    def collect_required(el, msg, out, path, top):
        for c in kids(el, *members_of):
            if c.get("presence") != "required":
                continue
            if top and is_envelope(c):
                continue
            if c.tag == FIXR + "fieldRef":
                out.add(ref_tag(c, f"message {msg}"))
            elif c.tag == FIXR + "groupRef":
                out.add(grp(c, f"message {msg}")[1])
            else:
                d = comp(c, f"message {msg}", path)
                collect_required(d, msg, out, path + [c.get("id")], False)

    def collect_allowed(el, msg, out, path, top):
        for c in kids(el, *members_of):
            if top and is_envelope(c):
                continue
            if c.tag == FIXR + "fieldRef":
                out.add(ref_tag(c, f"message {msg}"))
            elif c.tag == FIXR + "groupRef":
                g, counter = grp(c, f"message {msg}")
                out.add(counter)
                collect_allowed(g, msg, out, path, False)
            else:
                d = comp(c, f"message {msg}", path)
                collect_allowed(d, msg, out, path + [c.get("id")], False)

    def collect_members(el, ctx, out, path):
        for c in kids(el, *members_of):
            if c.tag == FIXR + "fieldRef":
                out.append(ref_tag(c, ctx))
            elif c.tag == FIXR + "groupRef":
                out.append(grp(c, ctx)[1])
            else:
                d = comp(c, ctx, path)
                collect_members(d, ctx, out, path + [c.get("id")])

    def collect_groups(el, mt, path, top):
        for c in kids(el, *members_of):
            if top and is_envelope(c):
                continue
            if c.tag == FIXR + "groupRef":
                g, counter = grp(c, f"message {mt}")
                members: list[int] = []
                collect_members(g, f"group {g.get('name')}", members, [])
                if not members:
                    raise DictError(f"group {g.get('name')} in message {mt} has no members")
                m.group_positions += 1
                key = (mt, counter)
                if key in m.groups and m.groups[key] != members:
                    raise DictError(f"counter {counter} twice in {mt}, different members")
                m.groups[key] = members
                m.group_def_keys.setdefault(g.get("id"), set()).add(key)
                collect_groups(g, mt, path, False)
            elif c.tag == FIXR + "componentRef":
                d = comp(c, f"message {mt}", path)
                collect_groups(d, mt, path + [c.get("id")], False)

    envelope_count: dict[str, int] = {}
    for msg in kids(section("messages"), "message"):
        name, mt = msg.get("name"), msg.get("msgType")
        if name is None or mt is None:
            raise DictError("<message> without name or msgType")
        if mt in m.msg_name:
            raise DictError(f"two messages share msgType {mt}")
        structure = msg.find(FIXR + "structure")
        if structure is None:
            raise DictError(f"message {name} has no <structure>")
        m.msg_name[mt] = name
        m.msg_admin[mt] = msg.get("category") == "Session"
        envelope_count[mt] = sum(1 for c in kids(structure, "componentRef") if is_envelope(c))
        req: set[int] = set()
        collect_required(structure, name, req, [], True)
        m.required[mt] = req
        allowed: set[int] = set()
        collect_allowed(structure, name, allowed, [], True)
        m.body[mt] = allowed
    for msg in kids(section("messages"), "message"):
        collect_groups(msg.find(FIXR + "structure"), msg.get("msgType"), [], True)
    collect_groups(header_el, "", [], False)

    odd = sorted(mt for mt, n in envelope_count.items() if n != 2)
    if odd:
        m.notes.append(f"messages without exactly one header + one trailer ref: {odd}")
    return m


# ---------------------------------------------------------------------------
# The acceptance corpus: which tags and message types the 59 .def files touch.
# ---------------------------------------------------------------------------


def read_defs() -> tuple[set[int], set[str], int]:
    files = sorted(glob.glob(os.path.join(DEF_DIR, "*.def")))
    tags: set[int] = set()
    msg_types: set[str] = set()
    for path in files:
        with open(path, "rb") as f:
            for raw in f.read().split(b"\n"):
                line = raw.rstrip(b"\r")
                if not line[:1] in (b"I", b"E"):
                    continue
                body = line[1:]
                # Multi-session lines: `I1,8=FIX.4.4...` — docs/reference/
                # quickfix-acceptance-def-format.md.
                mm = re.match(rb"^\d+,", body)
                if mm:
                    body = body[mm.end():]
                for tok in body.split(b"\x01"):
                    if b"=" not in tok:
                        continue
                    k, v = tok.split(b"=", 1)
                    if k.isdigit():
                        tags.add(int(k))
                        if k == b"35":
                            msg_types.add(v.decode("ascii", "replace"))
    return tags, msg_types, len(files)


# ---------------------------------------------------------------------------
# The nine-dimension comparison.
# ---------------------------------------------------------------------------


@dataclass
class Row:
    dim: str
    key: str
    left: str
    right: str
    tag: int | None = None
    msg_type: str | None = None
    gate_visible: bool = False
    why: str = ""


def diff(a: Model, b: Model) -> list[Row]:
    rows: list[Row] = []

    def fname(tag):
        return a.name_of.get(tag) or b.name_of.get(tag) or "?"

    def mname(mt):
        if mt == "":
            return "<header>"
        return a.msg_name.get(mt) or b.msg_name.get(mt) or "?"

    # T — tag number <-> name
    for tag in sorted(set(a.name_of) | set(b.name_of)):
        x, y = a.name_of.get(tag), b.name_of.get(tag)
        if x != y:
            rows.append(Row("T", f"tag {tag}", x or "absent", y or "absent", tag=tag))
    both_tags = set(a.name_of) & set(b.name_of)

    # Y — type variant, for tags on both sides
    for tag in sorted(both_tags):
        if a.type_of[tag] != b.type_of[tag]:
            rows.append(Row(
                "Y", f"{fname(tag)}({tag})",
                f"{a.type_of[tag]} [{a.raw_type_of[tag]}]",
                f"{b.type_of[tag]} [{b.raw_type_of[tag]}]", tag=tag,
            ))

    # M — message types
    for mt in sorted(set(a.msg_name) | set(b.msg_name)):
        if (mt in a.msg_name) != (mt in b.msg_name):
            rows.append(Row(
                "M", f"35={mt} {mname(mt)}",
                "present" if mt in a.msg_name else "absent",
                "present" if mt in b.msg_name else "absent", msg_type=mt,
            ))
    both_msgs = set(a.msg_name) & set(b.msg_name)

    # P — (message, tag) body pairs
    for mt in sorted(both_msgs):
        for tag in sorted(a.body[mt] ^ b.body[mt]):
            rows.append(Row(
                "P", f"35={mt} {mname(mt)} / {fname(tag)}({tag})",
                "present" if tag in a.body[mt] else "absent",
                "present" if tag in b.body[mt] else "absent",
                tag=tag, msg_type=mt,
            ))

    # R — required, where the pair exists on both sides
    for mt in sorted(both_msgs):
        on_both = a.body[mt] & b.body[mt]
        for tag in sorted((a.required[mt] ^ b.required[mt]) & on_both):
            rows.append(Row(
                "R", f"35={mt} {mname(mt)} / {fname(tag)}({tag})",
                "required" if tag in a.required[mt] else "optional",
                "required" if tag in b.required[mt] else "optional",
                tag=tag, msg_type=mt,
            ))

    # E — (tag, value) enum pairs, for tags on both sides
    for tag, val in sorted((a.enums ^ b.enums), key=lambda p: (p[0], p[1])):
        if tag not in both_tags:
            continue
        rows.append(Row(
            "E", f"{fname(tag)}({tag}) = {val!r}",
            "present" if (tag, val) in a.enums else "absent",
            "present" if (tag, val) in b.enums else "absent", tag=tag,
        ))

    # H — header and trailer membership
    for part, xa, xb in (("header", a.header, b.header), ("trailer", a.trailer, b.trailer)):
        for tag in sorted(xa ^ xb):
            rows.append(Row(
                "H", f"{part} / {fname(tag)}({tag})",
                "member" if tag in xa else "not member",
                "member" if tag in xb else "not member", tag=tag,
            ))

    # L — DATA -> length tag
    for tag in sorted(set(a.data_len) | set(b.data_len)):
        if tag not in both_tags:
            continue
        la, lb = a.data_len.get(tag, "not DATA"), b.data_len.get(tag, "not DATA")
        if la != lb:
            rows.append(Row("L", f"{fname(tag)}({tag})", str(la), str(lb), tag=tag))

    # G — (message, counter): exists, delimiter, member sequence exactly equal
    for key in sorted(set(a.groups) | set(b.groups)):
        mt, counter = key
        ga, gb = a.groups.get(key), b.groups.get(key)
        label = f"35={mt} {mname(mt)} / {fname(counter)}({counter})" if mt else \
            f"<header> / {fname(counter)}({counter})"
        if ga is None or gb is None:
            rows.append(Row(
                "G", label + " exists",
                "present" if ga is not None else "absent",
                "present" if gb is not None else "absent",
                tag=counter, msg_type=mt or None,
            ))
        elif ga[0] != gb[0]:
            rows.append(Row(
                "G", label + " delimiter",
                f"{fname(ga[0])}({ga[0]})", f"{fname(gb[0])}({gb[0]})",
                tag=counter, msg_type=mt or None,
            ))
        elif ga != gb:
            rows.append(Row(
                "G", label + " sequence", seq_str(ga, gb), seq_str(gb, ga),
                tag=counter, msg_type=mt or None,
            ))
    return rows


def seq_str(mine: list[int], other: list[int]) -> str:
    """The member sequence, with the first position that differs marked."""
    i = 0
    while i < min(len(mine), len(other)) and mine[i] == other[i]:
        i += 1
    lo = max(0, i - 1)
    shown = mine[lo:i + 3]
    s = " ".join(map(str, shown))
    return f"[{len(mine)} members] …@{i}: {s}{' …' if i + 3 < len(mine) else ''}"


def classify(rows: list[Row], def_tags: set[int], def_msgs: set[str]) -> None:
    gate_msgs = def_msgs | SESSION_MSG_TYPES
    for r in rows:
        if r.dim in ALWAYS_GATE_VISIBLE:
            r.gate_visible, r.why = True, f"every {r.dim} row"
        elif r.tag is not None and r.tag in def_tags:
            r.gate_visible, r.why = True, f"tag {r.tag} in the .def corpus"
        elif r.msg_type is not None and r.msg_type in SESSION_MSG_TYPES:
            r.gate_visible, r.why = True, f"35={r.msg_type} is a session message"
        elif r.msg_type is not None and r.msg_type in gate_msgs:
            r.gate_visible, r.why = True, f"35={r.msg_type} in the .def corpus"
        else:
            r.gate_visible, r.why = False, "quiet"


def outcome(rows: list[Row]) -> tuple[str, str]:
    """ADR-0101 decision 3, read in order; the first that holds wins.

    The program can decide A, and can decide C when B's arithmetic already
    fails. It cannot decide B: B needs every gate-visible row, and every row
    where the document might side against Orchestra, settled by the FIX 4.4
    Errata 20030618 text — the architect's work in plan row 1c. When B's
    arithmetic holds but A does not, the line says B is still open and C
    follows if the document review fails.
    """
    gv = sum(r.gate_visible for r in rows)
    quiet = len(rows) - gv
    total = len(rows)
    if gv == 0 and quiet <= A_QUIET_MAX:
        return "A", f"gate-visible = 0 and quiet = {quiet} <= {A_QUIET_MAX}"
    not_a = (f"not A: gate-visible = {gv}" if gv else
             f"not A: quiet = {quiet} > {A_QUIET_MAX}")
    if total > B_TOTAL_MAX:
        return "C", f"{not_a}; not B: total = {total} > {B_TOTAL_MAX}"
    return (
        "B-or-C",
        f"{not_a}; B's count holds (total = {total} <= {B_TOTAL_MAX}) — B iff the "
        f"FIX 4.4 Errata 20030618 settles all {gv} gate-visible rows and sides against "
        f"Orchestra on <= {B_OVERLAY_MAX} rows (plan row 1c); otherwise C",
    )


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------


def summary(rows: list[Row]) -> list[str]:
    out = ["facet  rows  gate-visible  quiet"]
    for d in DIMS:
        rs = [r for r in rows if r.dim == d]
        gv = sum(r.gate_visible for r in rs)
        out.append(f"{d:<5}  {len(rs):>4}  {gv:>12}  {len(rs) - gv:>5}")
    gv = sum(r.gate_visible for r in rows)
    out.append(f"total  {len(rows):>4}  {gv:>12}  {len(rows) - gv:>5}")
    return out


def table(rows: list[Row], left: str, right: str) -> list[str]:
    out = [f"dim | class | key | {left} | {right} | why"]
    for r in rows:
        cls = "GATE" if r.gate_visible else "quiet"
        out.append(f"{r.dim} | {cls} | {r.key} | {r.left} | {r.right} | {r.why}")
    return out


def counts_line(m: Model) -> str:
    c = m.counts()
    return "  ".join(f"{k}={c[k]}" for k in c) + f"  G-positions={m.group_positions}"


# ---------------------------------------------------------------------------
# Modes
# ---------------------------------------------------------------------------


def need(path: str, how: str) -> None:
    if not os.path.isfile(path):
        sys.exit(f"missing {os.path.relpath(path, REPO)} — run {how}")


def self_check() -> int:
    need(QF_XML, "scripts/fetch-quickfix-assets.sh")
    a = read_quickfix(QF_XML)
    b = read_quickfix(QF_XML)
    rows = diff(a, b)
    print("self-check: QuickFIX FIX44.xml against itself")
    print(f"  counts: {counts_line(a)}")
    print(f"  diff rows: {len(rows)}")
    ok = True
    if rows:
        ok = False
        print("  FAIL: a file compared with itself must give 0 rows")
        for line in table(rows[:20], "a", "b"):
            print("    " + line)
    got = a.counts()
    for k, want in EXPECTED_COUNTS.items():
        if got[k] != want:
            ok = False
            print(f"  FAIL: {k} is {got[k]}, the Rust tests assert {want}")
    if a.group_positions != EXPECTED_COUNTS["G"]:
        ok = False
        print(f"  FAIL: group positions {a.group_positions}, GROUP_POSITIONS is 731")
    # The Orchestra reader against itself too, when the file is there: a
    # reader that is not deterministic would show up here.
    if os.path.isfile(ORCH_XML):
        o = read_orchestra(ORCH_XML)
        orows = diff(o, read_orchestra(ORCH_XML))
        print("self-check: Orchestra against itself")
        print(f"  counts: {counts_line(o)}")
        print(f"  diff rows: {len(orows)}")
        if orows:
            ok = False
            print("  FAIL: a file compared with itself must give 0 rows")
    print("SELF-CHECK: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


def mutate(text: str, model: Model) -> tuple[str, str, str]:
    """Two deliberate edits to the Orchestra text. Returns (text, g_note, e_note).

    G: the first group definition (document order) whose flattening yields
    exactly ONE (msg, counter) key and whose 2nd and 3rd members are
    `fieldRef`s — those two are swapped, so the delimiter is unchanged and
    exactly one key's sequence differs.
    E: the first codeSet (document order) used by exactly ONE field and holding
    at least two codes — its last code is removed.
    """
    fref = r'<fixr:fieldRef id="(\d+)"[^>]*?(?:/>|>.*?</fixr:fieldRef>)'
    g_note = e_note = None
    for gm in re.finditer(r'<fixr:group id="(\d+)" name="([^"]+)".*?</fixr:group>', text, re.S):
        gid, gname = gm.group(1), gm.group(2)
        if len(model.group_def_keys.get(gid, ())) != 1:
            continue
        body = gm.group(0)
        members = list(re.finditer(
            r'<fixr:(fieldRef|groupRef|componentRef) id="(\d+)"[^>]*?(?:/>|>.*?</fixr:\1>)',
            body, re.S))
        if len(members) < 3 or members[1].group(1) != "fieldRef" or members[2].group(1) != "fieldRef":
            continue
        m1, m2 = members[1], members[2]
        between = body[m1.end():m2.start()]
        new_body = body[:m1.start()] + m2.group(0) + between + m1.group(0) + body[m2.end():]
        text = text[:gm.start()] + new_body + text[gm.end():]
        (key,) = model.group_def_keys[gid]
        g_note = (f"group {gname} (id {gid}, key 35={key[0]} counter {key[1]}): "
                  f"swapped members {m1.group(2)} and {m2.group(2)}")
        break
    for cm in re.finditer(r'<fixr:codeSet id="(\d+)" name="([^"]+)".*?</fixr:codeSet>', text, re.S):
        name = cm.group(2)
        if len(model.codeset_fields.get(name, ())) != 1:
            continue
        body = cm.group(0)
        codes = list(re.finditer(r'<fixr:code id="\d+" name="[^"]*" value="([^"]*)"[^>]*?(?:/>|>.*?</fixr:code>)',
                                 body, re.S))
        if len(codes) < 2:
            continue
        last = codes[-1]
        new_body = body[:last.start()] + body[last.end():]
        text = text[:cm.start()] + new_body + text[cm.end():]
        e_note = (f"codeSet {name} (field {model.codeset_fields[name][0]}): "
                  f"removed code value {last.group(1)!r}")
        break
    if g_note is None or e_note is None:
        raise DictError("mutation targets not found")
    return text, g_note, e_note


def mutation_check() -> int:
    need(ORCH_XML, "scripts/fetch-orchestra-assets.sh")
    orig = read_orchestra(ORCH_XML)
    with open(ORCH_XML, encoding="utf-8") as f:
        text = f.read()
    mutated, g_note, e_note = mutate(text, orig)
    os.makedirs(OUT_DIR, exist_ok=True)
    mpath = os.path.join(OUT_DIR, "OrchestraFIX44.mutated.xml")
    with open(mpath, "w", encoding="utf-8") as f:
        f.write(mutated)
    mut = read_orchestra(mpath)
    rows = diff(orig, mut)
    if os.path.isdir(DEF_DIR):
        def_tags, def_msgs, _ = read_defs()
        classify(rows, def_tags, def_msgs)
    print("mutation-check: Orchestra against a copy with two deliberate edits")
    print(f"  edit 1 (G): {g_note}")
    print(f"  edit 2 (E): {e_note}")
    print(f"  diff rows: {len(rows)}")
    for line in table(rows, "orchestra", "mutated"):
        print("    " + line)
    dims = sorted(r.dim for r in rows)
    ok = len(rows) == 2 and dims == ["E", "G"]
    if not ok:
        print(f"  FAIL: expected exactly 2 rows, one G and one E; got {dims}")
    print("MUTATION-CHECK: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


def real_run() -> int:
    need(QF_XML, "scripts/fetch-quickfix-assets.sh")
    need(ORCH_XML, "scripts/fetch-orchestra-assets.sh")
    got_sha = sha256_of(ORCH_XML)
    if got_sha != ORCH_PINNED_SHA256:
        sys.exit(f"sha256 mismatch: {ORCH_XML} is {got_sha}, ADR-0101 pins {ORCH_PINNED_SHA256}")
    qf = read_quickfix(QF_XML)
    orch = read_orchestra(ORCH_XML)
    def_tags, def_msgs, n_defs = read_defs()
    if n_defs != DEF_FILES_EXPECTED:
        sys.exit(f"{n_defs} .def files, ADR-0101 reads {DEF_FILES_EXPECTED}")
    rows = diff(qf, orch)
    classify(rows, def_tags, def_msgs)
    verdict, reason = outcome(rows)

    lines: list[str] = []
    lines.append("dict-diff: QuickFIX FIX 4.4 vs Orchestra FIX 4.4 (ADR-0101)")
    lines.append(f"  QuickFIX : {os.path.relpath(QF_XML, REPO)} sha256 {sha256_of(QF_XML)}")
    lines.append(f"  Orchestra: {os.path.relpath(ORCH_XML, REPO)} sha256 {got_sha}")
    lines.append(f"  counts QuickFIX : {counts_line(qf)}")
    lines.append(f"  counts Orchestra: {counts_line(orch)}")
    lines.append(f"  corpus: {n_defs} .def files, {len(def_tags)} distinct tags, "
                 f"{len(def_msgs)} message types {sorted(def_msgs)}")
    lines.append("")
    lines.append("DIVERGENCE TABLE")
    lines.extend(table(rows, "QuickFIX", "Orchestra"))
    lines.append("")
    lines.append("INFORMATIONAL — outside the nine facets, not counted by the rule")
    for mt in sorted(set(qf.msg_name) & set(orch.msg_name)):
        if qf.msg_name[mt] != orch.msg_name[mt]:
            lines.append(f"  message name 35={mt}: QuickFIX {qf.msg_name[mt]} / Orchestra {orch.msg_name[mt]}")
        if qf.msg_admin[mt] != orch.msg_admin[mt]:
            lines.append(f"  admin 35={mt}: QuickFIX msgcat admin={qf.msg_admin[mt]} / "
                         f"Orchestra category=Session {orch.msg_admin[mt]}")
    for tag in sorted(qf.header_required ^ orch.header_required):
        lines.append(f"  header required {qf.name_of.get(tag)}({tag}): QuickFIX "
                     f"{tag in qf.header_required} / Orchestra {tag in orch.header_required}")
    for tag in sorted(set(orch.data_len) | set(orch.data_len_by_name)):
        if orch.data_len.get(tag) != orch.data_len_by_name.get(tag):
            lines.append(f"  Orchestra lengthId vs name rule, {orch.name_of[tag]}({tag}): "
                         f"lengthId {orch.data_len.get(tag)} / name rule {orch.data_len_by_name.get(tag)}")
    for n in orch.notes:
        lines.append(f"  note: {n}")
    lines.append("")
    lines.append("SUMMARY")
    lines.extend(summary(rows))
    gv = sum(r.gate_visible for r in rows)
    lines.append(f"gate-visible: {gv}")
    lines.append(f"quiet: {len(rows) - gv}")
    lines.append(f"OUTCOME: {verdict} — {reason}")
    print("\n".join(lines))

    os.makedirs(OUT_DIR, exist_ok=True)
    with open(os.path.join(OUT_DIR, "report.md"), "w", encoding="utf-8") as f:
        f.write("# dict-diff report\n\n```text\n" + "\n".join(lines) + "\n```\n")
    with open(os.path.join(OUT_DIR, "report.json"), "w", encoding="utf-8") as f:
        json.dump({
            "quickfix_sha256": sha256_of(QF_XML),
            "orchestra_sha256": got_sha,
            "counts": {"quickfix": qf.counts(), "orchestra": orch.counts()},
            "rows": [r.__dict__ for r in rows],
            "gate_visible": gv,
            "quiet": len(rows) - gv,
            "outcome": verdict,
            "reason": reason,
        }, f, indent=1)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--self-check", action="store_true")
    g.add_argument("--mutation-check", action="store_true")
    args = ap.parse_args()
    try:
        if args.self_check:
            return self_check()
        if args.mutation_check:
            return mutation_check()
        return real_run()
    except DictError as e:
        print(f"dict-diff: refusing — {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
