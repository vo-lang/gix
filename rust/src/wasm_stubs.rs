// gix WASM stubs — RawCall returns ERR_NOT_SUPPORTED.
//
// gix is a native-only library and cannot run in the browser sandbox.
// These stubs allow the Vo shell handler bytecode to load in WASM without
// panicking on unregistered externs. Every gix call returns a descriptive
// error directing the user to the desktop app.
//
// Extern name: github_com_vo_lang_gix_RawCall

use vo_runtime::bytecode::ExternDef;
use vo_runtime::ffi::{ExternCallContext, ExternRegistry, ExternResult};
use vo_runtime::builtins::error_helper::write_error_to;

const MSG: &str = "git operations are not available in the browser sandbox \u{2014} \
                   use Vibe Studio desktop";

const PFX: &str = "github_com_vo_lang_gix_";

// RawCall(op string, input string) ([]byte, error)
// Vo return: ref at slot 0, error at slot 1

fn stub_raw_call(call: &mut ExternCallContext) -> ExternResult {
    call.ret_nil(0);
    write_error_to(call, 1, MSG);
    ExternResult::Ok
}

pub fn register_externs(registry: &mut ExternRegistry, externs: &[ExternDef]) {
    for (id, def) in externs.iter().enumerate() {
        let Some(func) = def.name.strip_prefix(PFX) else { continue };
        match func {
            "RawCall" => registry.register(id as u32, stub_raw_call),
            _ => {}
        }
    }
}
