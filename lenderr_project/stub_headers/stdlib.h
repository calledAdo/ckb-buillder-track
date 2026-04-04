/*
 * Minimal stdlib.h stub for RISC-V bare-metal clang compilation.
 */
#pragma once

typedef __SIZE_TYPE__ size_t;

static inline void abort(void) { __builtin_trap(); }
