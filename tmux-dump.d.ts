export type TmuxDump = TmuxSessionDump | TmuxSessionsDump;

export interface TmuxSessionsDump {
  sessions: TmuxSessionDump[];
}

export interface TmuxSessionDump {
  id?: string;
  name?: string;
  session_name?: string;
  created?: number | string;
  attached?: boolean;
  windows_count?: number | string;
  size?: TmuxSize;
  windows?: TmuxWindow[];
  error?: string;
}

export interface TmuxWindow {
  id?: string;
  index?: number | string;
  name?: string;
  active?: boolean;
  zoomed?: boolean;
  automatic_rename?: string;
  panes_count?: number | string;
  layout?: string;
  size?: TmuxSize;
  panes?: TmuxPane[];
}

export interface TmuxPane {
  id?: string;
  index?: number | string;
  title?: string;
  active?: boolean;
  dead?: boolean;
  geometry?: TmuxGeometry;
  tty?: string;
  pid?: number | string;
  path?: string;
  processes?: TmuxProcess[];
}

export interface TmuxProcess {
  pid: number;
  ppid: number | null;
  user: string;
  state: string;
  etime: string;
  command: string[];
}

export interface TmuxSize {
  width?: number | string;
  height?: number | string;
}

export interface TmuxGeometry {
  width?: number | string;
  height?: number | string;
  left?: number | string;
  top?: number | string;
}
