"""LLDB helper for extracting the current user's macOS QQ NT database key.

Adapted from QQBackup/qq-win-db-key's MIT-licensed macOS ARM helper. QQFlow
supplies the wrapper/result paths through environment variables and performs
database validation in Rust before accepting the captured value.
"""

import lldb
import os
import struct


_func_va = None
WRAPPER_PATH = os.environ.get(
    "QQFLOW_WRAPPER_PATH",
    "/Applications/QQ.app/Contents/Resources/app/wrapper.node",
)
KEY_RESULT_PATH = os.environ.get("QQFLOW_KEY_RESULT_PATH", "/tmp/qqflow_key_result")
BP_RESULT_PATH = os.environ.get("QQFLOW_BP_RESULT_PATH", "/tmp/qqflow_bp_result")


def _find_func_va(path):
    with open(path, "rb") as f:
        data = f.read()

    if len(data) < 8 or struct.unpack(">I", data[:4])[0] != 0xCAFEBABE:
        return None, "wrapper.node is not a universal Mach-O binary"

    arm64_off = None
    for i in range(struct.unpack(">I", data[4:8])[0]):
        base = 8 + i * 20
        if base + 20 > len(data):
            break
        if struct.unpack(">i", data[base : base + 4])[0] == 0x0100000C:
            arm64_off = struct.unpack(">I", data[base + 8 : base + 12])[0]
            break
    if arm64_off is None:
        return None, "wrapper.node has no arm64 slice"

    image = data[arm64_off:]
    if len(image) < 32:
        return None, "invalid arm64 Mach-O slice"
    command_offset = 32
    text_vmaddr = text_fileoff = text_size = 0
    for _ in range(struct.unpack("<I", image[16:20])[0]):
        if command_offset + 8 > len(image):
            break
        command, command_size = struct.unpack(
            "<II", image[command_offset : command_offset + 8]
        )
        if command == 0x19:  # LC_SEGMENT_64
            section_offset = command_offset + 72
            section_count = struct.unpack(
                "<I", image[command_offset + 64 : command_offset + 68]
            )[0]
            for _ in range(section_count):
                section_name = image[section_offset : section_offset + 16].rstrip(b"\0")
                segment_name = image[section_offset + 16 : section_offset + 32].rstrip(b"\0")
                if section_name == b"__text" and segment_name == b"__TEXT":
                    text_vmaddr, text_size = struct.unpack(
                        "<QQ", image[section_offset + 32 : section_offset + 48]
                    )
                    text_fileoff = struct.unpack(
                        "<I", image[section_offset + 48 : section_offset + 52]
                    )[0]
                section_offset += 80
        command_offset += command_size

    text = image[text_fileoff : text_fileoff + text_size]
    marker_a = image.find(b"nt_sqlite3_key_v2: db=")
    marker_b = image.find(b"nt_sqlite3_key_v2: no key")
    if marker_a < 0 or marker_b < 0:
        return None, "nt_sqlite3_key_v2 markers not found"

    # Scan __text once. The original research helper recomputed the full scan
    # for every outer match, which becomes prohibitively slow on QQ 6.9.99's
    # much larger wrapper.node.
    immediate_a = marker_a & 0xFFF
    immediate_b = marker_b & 0xFFF
    hits_a = []
    hits_b = []
    words = memoryview(text[: len(text) // 4 * 4]).cast("I")
    for index, instruction in enumerate(words):
        if instruction & 0xFFC00000 != 0x91000000:
            continue
        immediate = instruction >> 10 & 0xFFF
        if immediate == immediate_a:
            hits_a.append(index * 4)
        if immediate == immediate_b:
            hits_b.append(index * 4)

    for hit_a in hits_a:
        for hit_b in hits_b:
            if abs(hit_a - hit_b) >= 4096:
                continue
            start = min(hit_a, hit_b)
            for back in range(0, min(start, 2048), 4):
                pos = start - back
                instruction = struct.unpack("<I", text[pos : pos + 4])[0]
                if instruction & 0xFF8003FF == 0xD10003FF:
                    return text_vmaddr + pos, None
    return None, "nt_sqlite3_key_v2 function entry not found"


def _key_callback(frame, _bp_loc, _extra_args, _internal_dict):
    process = frame.GetThread().GetProcess()
    key_pointer = frame.FindRegister("x2")
    key_length = frame.FindRegister("x3")
    if not key_pointer.IsValid() or not key_length.IsValid():
        return False

    length = key_length.GetValueAsUnsigned()
    if length <= 0 or length > 128:
        return False
    error = lldb.SBError()
    raw = process.ReadMemory(key_pointer.GetValueAsUnsigned(), length, error)
    if error.Success() and raw:
        try:
            key = raw.decode("ascii")
        except Exception:
            key = raw.hex()
        with open(KEY_RESULT_PATH, "w", encoding="utf-8") as f:
            f.write(key)
    return False


def set_breakpoint(debugger, _command, result, _internal_dict):
    if _func_va is None:
        result.SetError("function address was not initialized")
        return
    target = debugger.GetSelectedTarget()
    for index in range(target.GetNumModules()):
        module = target.GetModuleAtIndex(index)
        if module.GetFileSpec().GetFilename() != "wrapper.node":
            continue
        load_address = module.GetObjectFileHeaderAddress().GetLoadAddress(target)
        if load_address == lldb.LLDB_INVALID_ADDRESS:
            continue
        breakpoint = target.BreakpointCreateByAddress(load_address + _func_va)
        if not breakpoint.IsValid():
            result.SetError("could not create nt_sqlite3_key_v2 breakpoint")
            return
        breakpoint.SetScriptCallbackFunction("qq_key_extractor_macos._key_callback")
        with open(BP_RESULT_PATH, "w", encoding="utf-8") as f:
            f.write(hex(load_address + _func_va))
        return
    result.SetError("wrapper.node is not loaded")


def __lldb_init_module(debugger, _internal_dict):
    global _func_va
    address, error = _find_func_va(WRAPPER_PATH)
    if address is None:
        print("[qqflow-key] " + str(error), flush=True)
        return
    _func_va = address
    debugger.HandleCommand(
        "command script add -f qq_key_extractor_macos.set_breakpoint qqflow-set-key-breakpoint"
    )
