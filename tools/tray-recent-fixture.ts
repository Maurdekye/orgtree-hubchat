import type { RecentSnapshot } from "../src/trayrecent-view";
export function recentFixture(): RecentSnapshot {
  return { generation: 1, revision: 1, rows: [], dark: true, submenuLeft: true,
    maxWidth:1920, maxHeight:1040, kind:"recent", recentAvailable:true,
    recent: [
      {peer:"person.111111",name:"Maya",kind:"person",unread:2,typing:false,picture:null,pictureKey:null},
      {peer:"org.222222",name:"Orgtree",kind:"org",unread:3,typing:true,picture:null,pictureKey:null},
      {peer:"chat.333333",name:"Research session",kind:"chat",unread:0,typing:false,picture:null,pictureKey:null},
      {peer:"group.444444",name:"Design team",kind:"person",unread:0,typing:false,picture:null,pictureKey:null},
      {peer:"org.555555",name:"A long organization name that must ellipsize",kind:"org",unread:120,typing:false,picture:null,pictureKey:null},
      {peer:"person.666666",name:"No unread",kind:"person",unread:0,typing:false,picture:null,pictureKey:null},
    ] };
}
