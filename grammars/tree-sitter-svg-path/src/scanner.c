#include "tree_sitter/parser.h"

// External scanner for tree-sitter-svg-path.
//
// `_number_continuation` is a zero-or-more-width lookahead token: it consumes
// an optional wsp / comma / wsp separator and commits ONLY when a number
// follows. This gives the GLR parser LR(k) lookahead so a command's trailing
// argument repeat (`C ... ... ...`) does not collapse into implicit linetos.
// Identical semantics to the host tree-sitter-svg scanner's NUMBER_CONTINUATION.

enum TokenType {
  NUMBER_CONTINUATION,
};

static inline bool is_wsp(int32_t c) {
  return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

static inline bool is_ascii_digit(int32_t c) {
  return c >= '0' && c <= '9';
}

void *tree_sitter_svg_path_external_scanner_create(void) { return NULL; }
void tree_sitter_svg_path_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_svg_path_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_svg_path_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

bool tree_sitter_svg_path_external_scanner_scan(void *payload, TSLexer *lexer,
                                                const bool *valid_symbols) {
  (void)payload;

  if (!valid_symbols[NUMBER_CONTINUATION]) {
    return false;
  }

  while (is_wsp(lexer->lookahead)) {
    lexer->advance(lexer, false);
  }

  if (lexer->lookahead == ',') {
    lexer->advance(lexer, false);
    while (is_wsp(lexer->lookahead)) {
      lexer->advance(lexer, false);
    }
  }

  int32_t c = lexer->lookahead;
  if (is_ascii_digit(c) || c == '+' || c == '-' || c == '.') {
    lexer->mark_end(lexer);
    lexer->result_symbol = NUMBER_CONTINUATION;
    return true;
  }

  return false;
}
