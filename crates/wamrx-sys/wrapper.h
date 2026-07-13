/*
 * Bindgen entry point for the WAMR embedding C API.
 *
 * We only bind the public embedding header `wasm_export.h`. It transitively
 * pulls in `lib_export.h` (for `NativeSymbol`) and defines `wasm_val_t` and the
 * value-kind enum, so there is no need to include `wasm_c_api.h` (which would
 * drag in symbols we do not use for the fast interpreter).
 */
#include "wasm_export.h"
