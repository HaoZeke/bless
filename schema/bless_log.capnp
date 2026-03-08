@0xa1b2c3d4e5f6a7b8;

struct LogLine {
  timestamp @0 :Float64;
  level @1 :Level;
  message @2 :Text;

  enum Level {
    trace @0;
    debug @1;
    info @2;
    warn @3;
    error @4;
  }
}

struct SessionMeta {
  label @0 :Text;
  uuid @1 :Text;
  command @2 :Text;
  args @3 :Text;
  hostname @4 :Text;
  startTime @5 :Float64;
}

struct SessionSummary {
  uuid @0 :Text;
  label @1 :Text;
  command @2 :Text;
  duration @3 :Text;
  lineCount @4 :UInt64;
  exitCode @5 :Int32;
}

interface BlessServer {
  openSession @0 (meta :SessionMeta) -> (sink :LogSink);
  listSessions @1 (limit :UInt32) -> (sessions :List(SessionSummary));
}

interface LogSink {
  writeBatch @0 (lines :List(LogLine)) -> ();
  close @1 (exitCode :Int32, duration :Text) -> ();
}
