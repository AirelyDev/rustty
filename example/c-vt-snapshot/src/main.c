#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <rustty/vt.h>

//! [snapshot-buffer-reader]
typedef struct {
  const uint8_t *data;
  size_t len;
  size_t offset;
} BufferReader;

// RusttyReader callbacks are synchronous. A successful zero-byte read is
// permanent EOF; returning false would report an I/O error.
static bool buffer_read(void *userdata,
                        uint8_t *buffer,
                        size_t capacity,
                        size_t *out_read) {
  BufferReader *reader = userdata;
  size_t remaining = reader->len - reader->offset;
  size_t count = remaining < capacity ? remaining : capacity;

  // Deliberately return short reads to demonstrate that the decoder retries.
  if (count > 64) count = 64;
  memcpy(buffer, reader->data + reader->offset, count);
  reader->offset += count;
  *out_read = count;
  return true;
}
//! [snapshot-buffer-reader]

int main(void) {
  RusttyResult result;

  //! [snapshot-encode]
  RusttyTerminal source = NULL;
  // A wide, shallow screen fills backing pages quickly enough to leave older
  // PAGE records after READY for the incremental decoder to demonstrate.
  result = rustty_terminal_new(NULL, &source, 215, 2);
  assert(result == RUSTTY_SUCCESS);

  // Snapshot encoding requires continuation tracking to be enabled before
  // feeding input. The limit bounds unfinished VT sequence retention.
  const size_t continuation_limit = 1024;
  result = rustty_terminal_set(
      source,
      RUSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES,
      &continuation_limit);
  assert(result == RUSTTY_SUCCESS);

  // Keep enough scrollback to demonstrate incremental history restoration.
  result = rustty_terminal_set(
      source, RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, NULL);
  assert(result == RUSTTY_SUCCESS);

  const char *line = "snapshot history line\r\n";
  for (size_t i = 0; i < 1000; i++) {
    rustty_terminal_vt_write(
        source, (const uint8_t *)line, strlen(line));
  }

  // Leave an SGR sequence unfinished so its continuation is snapshotted too.
  const char *unfinished = "\x1b[31";
  rustty_terminal_vt_write(
      source, (const uint8_t *)unfinished, strlen(unfinished));

  uint8_t *snapshot = NULL;
  size_t snapshot_len = 0;
  result = rustty_snapshot_encode_alloc(
      source, NULL, &snapshot, &snapshot_len);
  assert(result == RUSTTY_SUCCESS);
  printf("encoded %zu snapshot bytes\n", snapshot_len);
  //! [snapshot-encode]

  //! [snapshot-decode]
  RusttySnapshotDecoder full_decoder = NULL;
  result = rustty_snapshot_decoder_new_buf(
      NULL, &full_decoder, snapshot, snapshot_len);
  assert(result == RUSTTY_SUCCESS);

  RusttyTerminal full_terminal = NULL;
  result = rustty_snapshot_decoder_decode(full_decoder, &full_terminal);
  assert(result == RUSTTY_SUCCESS);

  rustty_snapshot_decoder_free(full_decoder);
  rustty_terminal_free(full_terminal);
  //! [snapshot-decode]

  //! [snapshot-compress-history]
  RusttySnapshotDecoder compressed_decoder = NULL;
  result = rustty_snapshot_decoder_new_buf(
      NULL, &compressed_decoder, snapshot, snapshot_len);
  assert(result == RUSTTY_SUCCESS);

  // Options must be set before decoding starts. With this set, each history
  // page is compressed as soon as it is restored, so the full scrollback is
  // never held uncompressed in memory.
  const bool compress_history = true;
  result = rustty_snapshot_decoder_set(
      compressed_decoder,
      RUSTTY_SNAPSHOT_DECODER_OPT_COMPRESS_HISTORY,
      &compress_history);
  assert(result == RUSTTY_SUCCESS);

  RusttyTerminal compressed_terminal = NULL;
  result = rustty_snapshot_decoder_decode(
      compressed_decoder, &compressed_terminal);
  assert(result == RUSTTY_SUCCESS);

  rustty_snapshot_decoder_free(compressed_decoder);
  rustty_terminal_free(compressed_terminal);
  //! [snapshot-compress-history]

  //! [snapshot-incremental]
  BufferReader reader_state = {
      .data = snapshot,
      .len = snapshot_len,
      .offset = 0,
  };
  RusttyReader reader = {
      .read = buffer_read,
      .userdata = &reader_state,
  };

  RusttySnapshotDecoder incremental_decoder = NULL;
  result = rustty_snapshot_decoder_new(
      NULL, &incremental_decoder, reader);
  assert(result == RUSTTY_SUCCESS);

  // READY returns a validated, renderable terminal before old history.
  RusttyTerminal incremental_terminal = NULL;
  result = rustty_snapshot_decoder_ready(
      incremental_decoder, &incremental_terminal);
  assert(result == RUSTTY_SUCCESS);

  uint64_t history_rows = 0;
  result = rustty_snapshot_decoder_get(
      incremental_decoder,
      RUSTTY_SNAPSHOT_DECODER_DATA_HISTORY_ROWS_PRIMARY,
      &history_rows);
  assert(result == RUSTTY_SUCCESS);
  printf("snapshot advertises %llu primary history rows\n",
         (unsigned long long)history_rows);

  size_t page_count = 0;
  while ((result = rustty_snapshot_decoder_next(incremental_decoder)) ==
         RUSTTY_SUCCESS) {
    RusttyTerminalScreen screen;
    size_t rows = 0;
    uint32_t remaining = 0;
    const RusttySnapshotDecoderData keys[] = {
        RUSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_SCREEN,
        RUSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_ROWS,
        RUSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_REMAINING,
    };
    void *values[] = {&screen, &rows, &remaining};
    size_t written = 0;
    result = rustty_snapshot_decoder_get_multi(
        incremental_decoder,
        sizeof(keys) / sizeof(keys[0]),
        keys,
        values,
        &written);
    assert(result == RUSTTY_SUCCESS);
    assert(written == sizeof(keys) / sizeof(keys[0]));

    printf("restored %zu rows to screen %d (%u pages remain)\n",
           rows, (int)screen, remaining);
    page_count++;
  }

  // NO_VALUE means FINISH validated successfully and is idempotent.
  assert(result == RUSTTY_NO_VALUE);
  assert(page_count > 0);
  assert(rustty_snapshot_decoder_next(incremental_decoder) ==
         RUSTTY_NO_VALUE);

  rustty_snapshot_decoder_free(incremental_decoder);
  rustty_terminal_free(incremental_terminal);
  //! [snapshot-incremental]

  rustty_free(NULL, snapshot, snapshot_len);
  rustty_terminal_free(source);
  return 0;
}
