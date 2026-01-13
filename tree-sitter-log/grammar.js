// tree-sitter-log - Grammar for aimax structured logs (s-expression format)
//
// Format:
//   (log LEVEL MODULE EVENT "TIMESTAMP" ((key . "value") ...))
//
// Example:
//   (log info llm chat-start "2024-01-13T12:00:00" ((messages . "1")))
//   (log error llm chat-error "2024-01-13T12:00:01" ((error . "connection failed")))

module.exports = grammar({
  name: 'log',

  rules: {
    source_file: $ => repeat($.entry),

    entry: $ => seq(
      '(',
      'log',
      $.level,
      $.module,
      $.event,
      $.timestamp,
      $.fields,
      ')'
    ),

    level: $ => choice('debug', 'info', 'warn', 'error'),

    module: $ => /[a-z_]+/,

    event: $ => /[a-z_-]+/,

    timestamp: $ => /"[^"]*"/,

    fields: $ => seq('(', repeat($.field), ')'),

    field: $ => seq(
      '(',
      $.key,
      '.',
      $.value,
      ')'
    ),

    key: $ => /[a-z_-]+/,

    value: $ => choice(
      $.string,
      $.number,
      $.symbol
    ),

    string: $ => /"([^"\\]|\\.)*"/,

    number: $ => /\d+/,

    symbol: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,
  }
});
