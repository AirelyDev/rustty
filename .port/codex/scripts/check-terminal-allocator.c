#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(CHECK_RUSTTY)
#include <rustty/vt.h>
typedef RusttyAllocator AuditAllocator;
typedef RusttyAllocatorVtable AuditAllocatorVtable;
typedef RusttyTerminal AuditTerminal;
typedef RusttyResult AuditResult;
#define audit_terminal_new rustty_terminal_new
#define audit_terminal_free rustty_terminal_free
#else
#include <ghostty/vt.h>
typedef GhosttyAllocator AuditAllocator;
typedef GhosttyAllocatorVtable AuditAllocatorVtable;
typedef GhosttyTerminal AuditTerminal;
typedef GhosttyResult AuditResult;
#define audit_terminal_new ghostty_terminal_new
#define audit_terminal_free ghostty_terminal_free
#endif

typedef struct {
  size_t alloc_calls;
  size_t free_calls;
} AuditState;

static void *fail_alloc(void *opaque, size_t len, uint8_t alignment,
                        uintptr_t ret_addr) {
  AuditState *state = opaque;
  (void)len;
  (void)alignment;
  (void)ret_addr;
  state->alloc_calls += 1;
  return NULL;
}

static bool fail_resize(void *opaque, void *memory, size_t memory_len,
                        uint8_t alignment, size_t new_len,
                        uintptr_t ret_addr) {
  (void)opaque;
  (void)memory;
  (void)memory_len;
  (void)alignment;
  (void)new_len;
  (void)ret_addr;
  return false;
}

static void *fail_remap(void *opaque, void *memory, size_t memory_len,
                        uint8_t alignment, size_t new_len,
                        uintptr_t ret_addr) {
  (void)opaque;
  (void)memory;
  (void)memory_len;
  (void)alignment;
  (void)new_len;
  (void)ret_addr;
  return NULL;
}

static void count_free(void *opaque, void *memory, size_t memory_len,
                       uint8_t alignment, uintptr_t ret_addr) {
  AuditState *state = opaque;
  (void)memory;
  (void)memory_len;
  (void)alignment;
  (void)ret_addr;
  state->free_calls += 1;
}

int main(void) {
  AuditState state = {0};
  const AuditAllocatorVtable vtable = {
      .alloc = fail_alloc,
      .resize = fail_resize,
      .remap = fail_remap,
      .free = count_free,
  };
  const AuditAllocator allocator = {.ctx = &state, .vtable = &vtable};
  AuditTerminal terminal = NULL;
  const AuditResult result = audit_terminal_new(&allocator, &terminal, 80, 24);

  const bool terminal_nonnull = terminal != NULL;
  if (terminal != NULL) audit_terminal_free(terminal);
  printf("result=%d terminal_nonnull=%d alloc_calls=%zu free_calls=%zu\n",
         (int)result, terminal_nonnull, state.alloc_calls, state.free_calls);
  return 0;
}
