// tree-sitter-log - Grammar for aimax structured logs
//
// Format:
//   TIMESTAMP LEVEL MODULE EVENT KEY=VALUE KEY="VALUE" ...
//
// Example:
//   2024-01-13T12:00:00 info llm chat-start messages=1 provider=Anthropic
//   2024-01-13T12:00:01 error llm chat-error error="connection failed"

module.exports = grammar({
  name: 'log',

  rules: {
    source_file: $ => repeat($.entry),

    entry: $ => seq(
      $.timestamp,
      $.level,
      $.module,
      $.event,
      repeat($.field),
      /\n/
    ),

    timestamp: $ => /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/,

    level: $ => choice('debug', 'info', 'warn', 'error'),

    module: $ => /[a-z_]+/,

    event: $ => /[a-z_-]+/,

    field: $ => seq(
      $.key,
      '=',
      $.value
    ),

    key: $ => /[a-z_]+/,

    value: $ => choice(
      $.string,
      $.number,
      $.identifier
    ),

    string: $ => /"[^"]*"/,

    number: $ => /\d+/,

    identifier: $ => /[A-Za-z_][A-Za-z0-9_.-]*/,
  }
});
