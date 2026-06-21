#include "tree_sitter/parser.h"
#include "tree_sitter/array.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum TokenType {
  START_TAG_NAME,
  PATH_START_TAG_NAME,
  SCRIPT_START_TAG_NAME,
  STYLE_START_TAG_NAME,
  END_TAG_NAME,
  PATH_END_TAG_NAME,
  SCRIPT_END_TAG_NAME,
  STYLE_END_TAG_NAME,
  ERRONEOUS_END_TAG_NAME,
  RAW_TEXT,
  SELF_CLOSING_TAG_DELIMITER,
  CDATA_TEXT,
  NUMBER_CONTINUATION,
};

typedef Array(char) String;
typedef Array(String) TagStack;

static inline void advance(TSLexer *lexer) {
  lexer->advance(lexer, false);
}

static inline bool is_ascii_alpha(int32_t c) {
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
}

static inline bool is_ascii_digit(int32_t c) {
  return c >= '0' && c <= '9';
}

static inline bool is_name_start_char(int32_t c) {
  return c == ':' || c == '_' || c >= 0x80 || is_ascii_alpha(c);
}

static inline bool is_name_char(int32_t c) {
  return is_name_start_char(c) || c == '-' || c == '.' || is_ascii_digit(c);
}

static inline uint32_t local_name_start(const String *name) {
  uint32_t start = 0;
  for (uint32_t i = 0; i < name->size; i++) {
    if (name->contents[i] == ':') {
      start = i + 1;
    }
  }

  return start;
}

static inline bool local_name_eq(const String *name, const char *value) {
  uint32_t start = local_name_start(name);
  uint32_t local_size = name->size - start;
  size_t expected_size = strlen(value);

  if (local_size != expected_size) {
    return false;
  }

  return memcmp(name->contents + start, value, expected_size) == 0;
}

static inline bool is_svg_name(const String *name) {
  return local_name_eq(name, "svg");
}

static inline bool is_path_name(const String *name) {
  return local_name_eq(name, "path");
}

static inline bool is_script_name(const String *name) {
  return local_name_eq(name, "script");
}

static inline bool is_style_name(const String *name) {
  return local_name_eq(name, "style");
}

static inline bool string_eq(const String *a, const String *b) {
  if (a->size != b->size) {
    return false;
  }

  if (a->size == 0) {
    return true;
  }

  return memcmp(a->contents, b->contents, a->size) == 0;
}

static void string_destroy(String *s) {
  array_delete(s);
}

// Tag names stored as char (8-bit). The (char) cast from int32_t truncates
// code points above U+007F. Safe for SVG (all element names are ASCII) and
// matches tree-sitter-xml/html. To support non-ASCII XML names, change
// String to Array(int32_t) and update serialization.
static String scan_tag_name(TSLexer *lexer) {
  String name = array_new();

  if (!is_name_start_char(lexer->lookahead)) {
    return name;
  }

  array_push(&name, (char)lexer->lookahead);
  advance(lexer);

  while (is_name_char(lexer->lookahead)) {
    array_push(&name, (char)lexer->lookahead);
    advance(lexer);
  }

  return name;
}

static bool scan_start_tag_name(TagStack *tags, TSLexer *lexer, const bool *valid_symbols) {
  String name = scan_tag_name(lexer);
  if (name.size == 0) {
    string_destroy(&name);
    return false;
  }

  int32_t symbol = -1;
  bool is_root_start = tags->size == 0;

  if (is_path_name(&name) && valid_symbols[PATH_START_TAG_NAME]) {
    symbol = PATH_START_TAG_NAME;
  } else if (is_script_name(&name) && valid_symbols[SCRIPT_START_TAG_NAME]) {
    symbol = SCRIPT_START_TAG_NAME;
  } else if (is_style_name(&name) && valid_symbols[STYLE_START_TAG_NAME]) {
    symbol = STYLE_START_TAG_NAME;
  } else if (valid_symbols[START_TAG_NAME] && (!is_root_start || is_svg_name(&name))) {
    symbol = START_TAG_NAME;
  }

  if (symbol < 0) {
    string_destroy(&name);
    return false;
  }

  array_push(tags, name);
  lexer->result_symbol = symbol;
  return true;
}

static bool scan_end_tag_name(TagStack *tags, TSLexer *lexer, const bool *valid_symbols) {
  String name = scan_tag_name(lexer);
  if (name.size == 0) {
    string_destroy(&name);
    return false;
  }

  bool top_matches = tags->size > 0 && string_eq(array_back(tags), &name);

  if (top_matches) {
    int32_t symbol = -1;

    if (is_path_name(&name) && valid_symbols[PATH_END_TAG_NAME]) {
      symbol = PATH_END_TAG_NAME;
    } else if (is_script_name(&name) && valid_symbols[SCRIPT_END_TAG_NAME]) {
      symbol = SCRIPT_END_TAG_NAME;
    } else if (is_style_name(&name) && valid_symbols[STYLE_END_TAG_NAME]) {
      symbol = STYLE_END_TAG_NAME;
    } else if (valid_symbols[END_TAG_NAME]) {
      symbol = END_TAG_NAME;
    }

    if (symbol >= 0) {
      String last = array_pop(tags);
      string_destroy(&last);
      string_destroy(&name);
      lexer->result_symbol = symbol;
      return true;
    }
  }

  string_destroy(&name);

  if (valid_symbols[ERRONEOUS_END_TAG_NAME]) {
    lexer->result_symbol = ERRONEOUS_END_TAG_NAME;
    return true;
  }

  return false;
}

static bool scan_raw_text(TagStack *tags, TSLexer *lexer) {
  if (tags->size == 0) {
    return false;
  }

  const String *tag = array_back(tags);

  // Consume everything until we see `</` followed by the matching tag name
  bool has_content = false;

  while (lexer->lookahead != 0) {
    if (lexer->lookahead == '<') {
      lexer->mark_end(lexer);
      advance(lexer);

      if (lexer->lookahead == '/') {
        advance(lexer);

        // Check if the following characters match the tag name
        bool matches = true;
        for (uint32_t i = 0; i < tag->size; i++) {
          // Cast safe: tag names are ASCII (see scan_tag_name note)
          if ((char)lexer->lookahead != tag->contents[i]) {
            matches = false;
            break;
          }
          advance(lexer);
        }

        // Accept XML 1.0 whitespace (S ::= #x20 | #x9 | #xD | #xA) or '>'
        // as tag-name terminators.
        // '/' is NOT accepted: `</tag/>` is malformed XML.
        // EOF after `</name` is not a match — fall through and continue
        // consuming as raw text for error tolerance.
        if (matches && (lexer->lookahead == '>' || lexer->lookahead == ' ' ||
                        lexer->lookahead == '\t' || lexer->lookahead == '\r' ||
                        lexer->lookahead == '\n')) {
          // Found the closing tag — return the raw_text up to `</`
          if (has_content) {
            lexer->result_symbol = RAW_TEXT;
            return true;
          }
          return false;
        }
      }

      has_content = true;
    } else {
      has_content = true;
      advance(lexer);
    }
  }

  // EOF reached — emit whatever we consumed
  lexer->mark_end(lexer);
  if (has_content) {
    lexer->result_symbol = RAW_TEXT;
    return true;
  }
  return false;
}

// Scan one chunk of CDATA content. Called via repeat1() in the grammar.
// Returns false when ]]> is next, letting the grammar match the literal.
//
// When returning false, tree-sitter resets the lexer position to the
// scan start — the advance() calls are effectively undone.
//
// Emits one of:
//   - A run of non-] characters
//   - Exactly one ] (confirmed non-]]> by peeking two chars ahead)
//
// For ]]X (two ] not followed by >): emits one ] as content via
// mark_end, leaving the second ] to be re-lexed on the next call.
static bool scan_cdata_text(TSLexer *lexer) {
  if (lexer->lookahead == 0) {
    return false;
  }

  if (lexer->lookahead == ']') {
    advance(lexer);           // consume first ]
    lexer->mark_end(lexer);   // token boundary: after first ]

    if (lexer->lookahead == ']') {
      advance(lexer);         // consume second ] (past mark_end, re-lexed if true)

      if (lexer->lookahead == '>') {
        // ]]> found. Return false — lexer resets to before the first ],
        // grammar matches ]]> as a literal.
        return false;
      }

      // ]] not followed by >. First ] is content (mark_end covers it).
      // Second ] is past mark_end — gets re-lexed on next call.
      lexer->result_symbol = CDATA_TEXT;
      return true;
    }

    // Single ] — not start of ]]>, so it's content.
    lexer->result_symbol = CDATA_TEXT;
    return true;
  }

  // Consume run of non-] characters
  while (lexer->lookahead != 0 && lexer->lookahead != ']') {
    advance(lexer);
  }

  lexer->mark_end(lexer);
  lexer->result_symbol = CDATA_TEXT;
  return true;
}

static inline bool is_wsp(int32_t c) {
  return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

// Peek past optional comma/whitespace separator. If a number follows,
// consume the separator and return true. Otherwise return false (lexer
// resets). This gives path-segment repeats (arc, h, v, C, S, Q, T)
// LR(k) lookahead so the parser can commit to extending the current
// segment instead of reducing and falling through to either
// implicit_lineto_segment or a following explicit command.
static bool scan_number_continuation(TSLexer *lexer) {
  while (is_wsp(lexer->lookahead)) {
    advance(lexer);
  }

  if (lexer->lookahead == ',') {
    advance(lexer);
    while (is_wsp(lexer->lookahead)) {
      advance(lexer);
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

static bool scan_self_closing_tag_delimiter(TagStack *tags, TSLexer *lexer) {
  if (lexer->lookahead != '/') {
    return false;
  }

  advance(lexer);

  if (lexer->lookahead != '>') {
    return false;
  }

  advance(lexer);

  if (tags->size > 0) {
    String last = array_pop(tags);
    string_destroy(&last);
  }

  lexer->result_symbol = SELF_CLOSING_TAG_DELIMITER;
  return true;
}

void *tree_sitter_svg_external_scanner_create(void) {
  TagStack *tags = (TagStack *)ts_calloc(1, sizeof(TagStack));
  if (tags == NULL) {
    abort();
  }

  array_init(tags);
  return tags;
}

void tree_sitter_svg_external_scanner_destroy(void *payload) {
  TagStack *tags = (TagStack *)payload;
  for (uint32_t i = 0; i < tags->size; i++) {
    string_destroy(array_get(tags, i));
  }

  array_delete(tags);
  ts_free(tags);
}

unsigned tree_sitter_svg_external_scanner_serialize(void *payload, char *buffer) {
  TagStack *tags = (TagStack *)payload;

  uint16_t count = tags->size > UINT16_MAX ? UINT16_MAX : (uint16_t)tags->size;
  unsigned size = 0;

  if (TREE_SITTER_SERIALIZATION_BUFFER_SIZE < sizeof(uint16_t)) {
    return 0;
  }

  memcpy(&buffer[size], &count, sizeof(uint16_t));
  size += sizeof(uint16_t);

  uint16_t written = 0;

  // Serialize as many tags as fit. If buffer fills, stop early and patch
  // the count header. Truncated deep tags become erroneous_end_tag.
  // SVG nesting rarely exceeds the ~1024-byte buffer.
  for (uint16_t i = 0; i < count; i++) {
    String *tag = array_get(tags, i);
    uint8_t length = tag->size > UINT8_MAX ? UINT8_MAX : (uint8_t)tag->size;

    if (size + 1 + length > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) {
      break;
    }

    buffer[size++] = (char)length;
    if (length > 0) {
      memcpy(&buffer[size], tag->contents, length);
      size += length;
    }

    written++;
  }

  memcpy(&buffer[0], &written, sizeof(uint16_t));
  return size;
}

void tree_sitter_svg_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  TagStack *tags = (TagStack *)payload;

  for (uint32_t i = 0; i < tags->size; i++) {
    string_destroy(array_get(tags, i));
  }
  array_clear(tags);

  if (length < sizeof(uint16_t)) {
    return;
  }

  unsigned size = 0;
  uint16_t count = 0;
  memcpy(&count, &buffer[size], sizeof(uint16_t));
  size += sizeof(uint16_t);

  for (uint16_t i = 0; i < count; i++) {
    if (size >= length) {
      break;
    }

    uint8_t tag_length = (uint8_t)buffer[size++];
    if (size + tag_length > length) {
      break;
    }

    String tag = array_new();
    if (tag_length > 0) {
      array_reserve(&tag, tag_length);
      for (uint8_t j = 0; j < tag_length; j++) {
        array_push(&tag, buffer[size + j]);
      }
      size += tag_length;
    }

    array_push(tags, tag);
  }
}

bool tree_sitter_svg_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  TagStack *tags = (TagStack *)payload;

  // Only scan raw_text when genuinely inside script/style context,
  // not during error recovery (where all valid_symbols are true).
  if (valid_symbols[RAW_TEXT] && !valid_symbols[START_TAG_NAME] && !valid_symbols[END_TAG_NAME]) {
    return scan_raw_text(tags, lexer);
  }

  // CDATA text: scan content up to ]]> with correct boundary handling.
  // Guard against error recovery (all valid_symbols true).
  if (valid_symbols[CDATA_TEXT] && !valid_symbols[START_TAG_NAME] && !valid_symbols[END_TAG_NAME]) {
    return scan_cdata_text(lexer);
  }

  // Number continuation: peek past whitespace/comma; succeed only if a number follows.
  // Used by elliptical arc, horizontal lineto, and vertical lineto repeats to
  // commit to extension instead of falling through to implicit_lineto_segment.
  if (valid_symbols[NUMBER_CONTINUATION] && !valid_symbols[START_TAG_NAME] && !valid_symbols[END_TAG_NAME]) {
    return scan_number_continuation(lexer);
  }

  if (valid_symbols[SELF_CLOSING_TAG_DELIMITER] && lexer->lookahead == '/') {
    return scan_self_closing_tag_delimiter(tags, lexer);
  }

  bool any_start_valid =
      valid_symbols[START_TAG_NAME] ||
      valid_symbols[PATH_START_TAG_NAME] ||
      valid_symbols[SCRIPT_START_TAG_NAME] ||
      valid_symbols[STYLE_START_TAG_NAME];

  if (any_start_valid && scan_start_tag_name(tags, lexer, valid_symbols)) {
    return true;
  }

  bool any_end_valid =
      valid_symbols[END_TAG_NAME] ||
      valid_symbols[PATH_END_TAG_NAME] ||
      valid_symbols[SCRIPT_END_TAG_NAME] ||
      valid_symbols[STYLE_END_TAG_NAME] ||
      valid_symbols[ERRONEOUS_END_TAG_NAME];

  if (any_end_valid && scan_end_tag_name(tags, lexer, valid_symbols)) {
    return true;
  }

  return false;
}
