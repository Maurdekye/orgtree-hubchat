export interface DesktopActions {
  settings:()=>void;directory:()=>void;newChat:()=>void;hubs:()=>void;refresh:()=>void;
  cycleFilter:(step:1|-1)=>void;
}
const keys:Record<string,keyof Omit<DesktopActions,"cycleFilter">>={F1:"settings",F2:"directory",F3:"newChat",F4:"hubs",F5:"refresh"};
export function desktopShortcut(event:KeyboardEvent,actions:DesktopActions,blocked:boolean):boolean {
  const plain=!event.ctrlKey&&!event.altKey&&!event.metaKey&&!event.shiftKey;
  const cycle=event.altKey&&!event.ctrlKey&&!event.metaKey&&!event.shiftKey&&(event.key==="ArrowLeft"||event.key==="ArrowRight");
  const action=plain?keys[event.key]:undefined;
  if(!action&&!cycle)return false;
  event.preventDefault();event.stopImmediatePropagation();
  if(blocked||event.repeat)return true;
  if(cycle)actions.cycleFilter(event.key==="ArrowRight"?1:-1);else actions[action!]();
  return true;
}
