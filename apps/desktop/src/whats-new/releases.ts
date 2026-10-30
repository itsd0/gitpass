import {
  FileText,
  Globe,
  KeyRound,
  LayoutList,
  PanelLeftClose,
  Puzzle,
  QrCode,
  Search,
  ShieldAlert,
  ShieldCheck,
  Terminal,
} from 'lucide-react';
import type { ReleaseHighlights, WhatsNewHighlight } from './types';
import { compareSemver } from './semver';

/**
 * 0.6.6 highlights. 0.6.6 shipped before these were added (it fell back to the 0.6.5 sheet),
 * so 0.6.7 re-uses them: it is the first build that actually shows them.
 */
const HIGHLIGHTS_0_6_6: WhatsNewHighlight[] = [
  {
    id: 'search-operators',
    icon: Search,
    title: {
      en: 'Search Operators Everywhere',
      my: 'နေရာတိုင်းတွင် ရှာဖွေရေး အော်ပရေတာများ',
      th: 'ตัวดำเนินการค้นหาได้ทุกที่',
    },
    description: {
      en: 'Narrow searches with query operators in the app, the CLI and the browser extension.',
      my: 'အက်ပ်၊ CLI နှင့် ဘရောက်ဇာ extension တို့တွင် အော်ပရေတာများဖြင့် ရှာဖွေမှုကို ကျဉ်းမြောင်းနိုင်ပါပြီ။',
      th: 'จำกัดผลการค้นหาด้วยตัวดำเนินการในแอป CLI และส่วนขยายเบราว์เซอร์',
    },
    showMe: {
      route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
    },
  },
  {
    id: 'list-density-text-size',
    icon: LayoutList,
    title: {
      en: 'Density, Text Size and Icon Rail',
      my: 'စာရင်းသိပ်သည်းမှု၊ စာလုံးအရွယ်နှင့် အိုင်ကွန်ဘေးဘောင်',
      th: 'ความหนาแน่น ขนาดตัวอักษร และแถบไอคอน',
    },
    description: {
      en: 'Choose a compact or comfortable list, set the text size (⌘+ / ⌘− / ⌘0) and collapse the sidebar to an icon rail.',
      my: 'စာရင်းကို ကျစ်လျစ်သော သို့မဟုတ် ချောင်ချိသောပုံစံ ရွေးပါ၊ စာလုံးအရွယ် (⌘+ / ⌘− / ⌘0) သတ်မှတ်ပြီး ဘေးဘောင်ကို အိုင်ကွန်အဖြစ် ခေါက်သိမ်းနိုင်ပါသည်။',
      th: 'เลือกรายการแบบกะทัดรัดหรือแบบสบายตา ปรับขนาดตัวอักษร (⌘+ / ⌘− / ⌘0) และย่อแถบด้านข้างเป็นแถบไอคอน',
    },
    showMe: {
      route: { name: 'settings', tab: 'general' },
    },
  },
  {
    id: 'qr-codes',
    icon: QrCode,
    title: {
      en: 'QR Codes In and Out',
      my: 'QR ကုဒ် ပြသခြင်းနှင့် စကင်ဖတ်ခြင်း',
      th: 'แสดงและสแกนรหัส QR',
    },
    description: {
      en: 'Show item fields as a QR code, and scan a TOTP QR code from the screen, the clipboard or a dropped image.',
      my: 'ပစ္စည်းအကွက်များကို QR ကုဒ်အဖြစ် ပြသပြီး TOTP QR ကုဒ်ကို မျက်နှာပြင်၊ ကလစ်ဘုတ် သို့မဟုတ် ဆွဲချထားသောပုံမှ စကင်ဖတ်နိုင်ပါသည်။',
      th: 'แสดงฟิลด์ของรายการเป็นรหัส QR และสแกนรหัส QR ของ TOTP จากหน้าจอ คลิปบอร์ด หรือรูปภาพที่ลากมาวาง',
    },
  },
  {
    id: 'extension-archive-site-settings',
    icon: Puzzle,
    title: {
      en: 'Extension Archive and Site Settings',
      my: 'Extension မှတ်တမ်းဟောင်းနှင့် ဆိုက်ဆက်တင်များ',
      th: 'คลังเก็บและการตั้งค่าเว็บไซต์ในส่วนขยาย',
    },
    description: {
      en: 'The browser extension popup now has Archive and Recently Deleted views with restore, plus per-site settings.',
      my: 'ဘရောက်ဇာ extension popup တွင် Archive နှင့် မကြာသေးမီ ဖျက်ထားသည်များကို ပြန်ယူနိုင်ပြီး ဆိုက်တစ်ခုချင်းစီ ဆက်တင်များ ပါဝင်လာပါပြီ။',
      th: 'ป๊อปอัปของส่วนขยายเบราว์เซอร์มีมุมมองคลังเก็บและรายการที่ลบล่าสุดพร้อมการกู้คืน และการตั้งค่ารายเว็บไซต์',
    },
  },
  {
    id: 'ssh-commit-signing-cli',
    icon: Terminal,
    title: {
      en: 'SSH Commit Signing and CLI Commands',
      my: 'SSH ဖြင့် commit လက်မှတ်ထိုးခြင်းနှင့် CLI ညွှန်ကြားချက်များ',
      th: 'ลงนามคอมมิตด้วย SSH และคำสั่ง CLI ใหม่',
    },
    description: {
      en: 'Sign Git commits and tags with an SSH key from your vault, and manage items from the CLI: tag, favorite, archive, move and create more item types.',
      my: 'Vault ထဲရှိ SSH key ဖြင့် Git commit နှင့် tag များကို လက်မှတ်ထိုးပြီး CLI မှ ပစ္စည်းများကို tag၊ အကြိုက်ဆုံး၊ archive၊ ရွှေ့ခြင်းနှင့် အမျိုးအစားသစ်များ ဖန်တီးနိုင်ပါသည်။',
      th: 'ลงนามคอมมิตและแท็ก Git ด้วยคีย์ SSH จากห้องนิรภัย และจัดการรายการจาก CLI: ติดแท็ก รายการโปรด เก็บถาวร ย้าย และสร้างรายการประเภทใหม่',
    },
    showMe: {
      route: { name: 'settings', tab: 'ssh' },
    },
  },
  {
    id: 'unlock-protection-passkeys',
    icon: KeyRound,
    title: {
      en: 'Safer Unlock and Passkey Fix',
      my: 'ပိုလုံခြုံသော သော့ဖွင့်ခြင်းနှင့် Passkey ပြင်ဆင်မှု',
      th: 'ปลดล็อกปลอดภัยขึ้นและแก้ไขพาสคีย์',
    },
    description: {
      en: 'Repeated wrong master passwords now add an increasing delay, a Caps Lock hint warns you while typing, and sites can register passkeys created by gitpass again.',
      my: 'မာစတာစကားဝှက် အကြိမ်ကြိမ်မှားပါက စောင့်ချိန် တိုးလာမည်၊ Caps Lock ဖွင့်ထားလျှင် သတိပေးမည်ဖြစ်ပြီး gitpass ဖန်တီးသော passkey များကို ဆိုက်များတွင် မှတ်ပုံတင်နိုင်ပါပြီ။',
      th: 'การใส่รหัสผ่านหลักผิดซ้ำจะมีการหน่วงเวลาเพิ่มขึ้น มีคำเตือนเมื่อเปิด Caps Lock และเว็บไซต์สามารถลงทะเบียนพาสคีย์ที่ gitpass สร้างได้แล้ว',
    },
    showMe: {
      route: { name: 'settings', tab: 'security' },
    },
  },
];

export const RELEASES: ReleaseHighlights[] = [
  {
    version: '0.6.8',
    highlights: [
      {
        id: 'emergency-kit-view-save',
        icon: FileText,
        title: {
          en: 'View & Save Your Emergency Kit',
          my: 'Emergency Kit ကို ကြည့်ရှုပြီး သိမ်းဆည်းပါ',
          th: 'ดูและบันทึก Emergency Kit',
        },
        description: {
          en: 'Open or save your Emergency Kit any time without rotating your Secret Key. Printing no longer marks the kit as saved.',
          my: 'Secret Key ကို မပြောင်းဘဲ Emergency Kit ကို အချိန်မရွေး ဖွင့်ကြည့်ခြင်း သို့မဟုတ် သိမ်းဆည်းနိုင်ပါပြီ။ ပုံနှိပ်ခြင်းဖြင့် kit ကို သိမ်းပြီးဟု မှတ်သားတော့မည် မဟုတ်ပါ။',
          th: 'เปิดหรือบันทึก Emergency Kit ได้ทุกเมื่อโดยไม่ต้องเปลี่ยน Secret Key และการพิมพ์จะไม่ทำเครื่องหมายว่าบันทึก kit แล้วอีกต่อไป',
        },
        showMe: {
          route: { name: 'settings', tab: 'security' },
        },
      },
      {
        id: 'password-history',
        icon: KeyRound,
        title: {
          en: 'Password History & Rotation Reminders',
          my: 'စကားဝှက် မှတ်တမ်းနှင့် ပြောင်းလဲရန် သတိပေးချက်များ',
          th: 'ประวัติรหัสผ่านและการเตือนให้เปลี่ยนรหัสผ่าน',
        },
        description: {
          en: 'Logins keep their previous passwords, and you can set a per-login reminder to rotate them.',
          my: 'Login များတွင် ယခင်စကားဝှက်များကို သိမ်းထားပြီး login တစ်ခုချင်းစီအတွက် စကားဝှက်ပြောင်းရန် သတိပေးချက် သတ်မှတ်နိုင်ပါပြီ။',
          th: 'ล็อกอินจะเก็บรหัสผ่านเดิมไว้ และตั้งการเตือนให้เปลี่ยนรหัสผ่านแยกตามล็อกอินได้',
        },
        showMe: {
          route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
        },
      },
      {
        id: 'watchtower-history-row-icons',
        icon: ShieldCheck,
        title: {
          en: 'Watchtower Score History',
          my: 'Watchtower ရမှတ် မှတ်တမ်း',
          th: 'ประวัติคะแนน Watchtower',
        },
        description: {
          en: 'See your score trend with a weekly change, and spot items with findings right in the item list.',
          my: 'အပတ်စဉ် ပြောင်းလဲမှုနှင့်အတူ ရမှတ်လမ်းကြောင်းကို ကြည့်ပြီး ပြဿနာရှိသော item များကို စာရင်းထဲတွင် တန်းမြင်နိုင်ပါပြီ။',
          th: 'ดูแนวโน้มคะแนนพร้อมการเปลี่ยนแปลงรายสัปดาห์ และเห็นรายการที่มีปัญหาได้ทันทีในรายการ',
        },
        showMe: {
          route: { name: 'watchtower' },
        },
      },
      {
        id: 'extension-watchtower',
        icon: Puzzle,
        title: {
          en: 'Watchtower in the Extension',
          my: 'Extension ထဲရှိ Watchtower',
          th: 'Watchtower ในส่วนขยาย',
        },
        description: {
          en: 'The browser popup shows a vault-wide Watchtower overview and a severity dot, and lets you pick which login to update.',
          my: 'ဘရောက်ဇာ popup တွင် vault တစ်ခုလုံး၏ Watchtower အကျဉ်းချုပ်နှင့် အဆင့်ပြအစက်ကို ပြပြီး မည်သည့် login ကို update လုပ်မည်ကို ရွေးနိုင်ပါပြီ။',
          th: 'ป๊อปอัปเบราว์เซอร์แสดงภาพรวม Watchtower ทั้งห้องนิรภัยและจุดระดับความรุนแรง และให้เลือกล็อกอินที่จะอัปเดตได้',
        },
      },
    ],
  },
  { version: '0.6.7', highlights: HIGHLIGHTS_0_6_6 },
  { version: '0.6.6', highlights: HIGHLIGHTS_0_6_6 },
  {
    version: '0.6.5',
    highlights: [
      {
        id: 'markdown-notes',
        icon: FileText,
        title: {
          en: 'Safe Markdown Notes',
          my: 'လုံခြုံသော Markdown မှတ်စုများ',
          th: 'บันทึก Markdown ปลอดภัย',
        },
        description: {
          en: 'Format secure notes with rich Markdown rendering while keeping data encrypted and sanitized.',
          my: 'အချက်အလက်များကို လျှို့ဝှက်ကုဒ်ဖြင့် ကာကွယ်ထားပြီး Markdown ဖြင့် လုံခြုံစွာ ဖော်ပြနိုင်ပါပြီ။',
          th: 'จัดรูปแบบโน้ตที่ปลอดภัยด้วย Markdown พร้อมคงการเข้ารหัสและความปลอดภัยของข้อมูล',
        },
        showMe: {
          route: { name: 'settings', tab: 'general' },
        },
      },
      {
        id: 'screen-recording-protection',
        icon: ShieldAlert,
        title: {
          en: 'Screen Recording Protection',
          my: 'မျက်နှာပြင် မှတ်တမ်းတင်ခြင်း ကာကွယ်မှု',
          th: 'การป้องกันการบันทึกหน้าจอ',
        },
        description: {
          en: 'Automatically hides gitpass windows from screenshots, screen recorders, and video shares.',
          my: 'စခရင်မှတ်တမ်းတင်ခြင်းနှင့် ဗီဒီယိုမျှဝေမှုများမှ gitpass ဝင်းဒိုးကို အလိုအလျောက် ဝှက်ထားပေးပါသည်။',
          th: 'ซ่อนหน้าต่าง gitpass โดยอัตโนมัติจากการบันทึกภาพหน้าจอ การบันทึกวิดีโอหน้าจอ และการแชร์หน้าจอ',
        },
        showMe: {
          route: { name: 'settings', tab: 'security' },
        },
      },
      {
        id: 'watchtower-enhancements',
        icon: ShieldCheck,
        title: {
          en: 'Watchtower Password Health',
          my: 'Watchtower စကားဝှက် စစ်ဆေးမှု',
          th: 'ตรวจสุขภาพรหัสผ่านด้วย Watchtower',
        },
        description: {
          en: 'Audit your vault for weak passwords, reused credentials, missing 2FA, and compromised accounts.',
          my: 'အားနည်းသော စကားဝှက်များ၊ ထပ်ခါသုံးစကားဝှက်များနှင့် ပေါက်ကြားမှုများကို စစ်ဆေးပါ။',
          th: 'ตรวจสอบรหัสผ่านที่อ่อนแอ การใช้ซ้ำ การขาด 2FA และบัญชีที่รั่วไหล',
        },
        showMe: {
          route: { name: 'watchtower' },
        },
      },
      {
        id: 'sidebar-rail',
        icon: PanelLeftClose,
        title: {
          en: 'Collapsible Sidebar Rail',
          my: 'ခေါက်သိမ်းနိုင်သော ဘေးဘောင်',
          th: 'แถบด้านข้างแบบย่อเป็นไอคอน',
        },
        description: {
          en: 'Collapse the sidebar to a compact icon rail to maximize your viewing space (⌘\\ or Ctrl+\\).',
          my: 'ကြည့်ရှုရန် နေရာပိုမိုရရှိစေရန် ဘေးဘောင်ကို အိုင်ကွန်အသေးအဖြစ် ခေါက်သိမ်းနိုင်ပါပြီ (⌘\\ သို့မဟုတ် Ctrl+\\)။',
          th: 'ย่อแถบด้านข้างเป็นแถบไอคอนขนาดกะทัดรัดเพื่อเพิ่มพื้นที่มุมมอง (⌘\\ หรือ Ctrl+\\)',
        },
        showMe: {
          route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
        },
      },
      {
        id: 'website-favicons',
        icon: Globe,
        title: {
          en: 'Website Favicons',
          my: 'ဝက်ဘ်ဆိုက် သင်္ကေတများ',
          th: 'ไอคอนเว็บไซต์',
        },
        description: {
          en: 'Optional, privacy-preserving website favicons in your item list with offline caching.',
          my: 'ပစ္စည်းစာရင်းတွင် ဝက်ဘ်ဆိုက်သင်္ကေတများကို ကိုယ်ရေးလုံခြုံမှုရှိစွာ စိတ်ကြိုက်ပြသနိုင်ပါသည်။',
          th: 'แสดงไอคอนเว็บไซต์ในรายการรายการของคุณ พร้อมแคชออฟไลน์และคำนึงถึงความเป็นส่วนตัว',
        },
        showMe: {
          route: { name: 'settings', tab: 'general' },
        },
      },
    ],
  },
];

/**
 * Finds highlights for the specified version.
 * If exact version is not matched, returns the newest release available.
 */
export function getReleaseHighlights(version?: string): ReleaseHighlights | undefined {
  if (!version) return RELEASES[0];
  const clean = version.trim().replace(/^v/i, '');
  const match = RELEASES.find((r) => r.version === clean);
  if (match) return match;

  // Fallback to highest version defined
  const sorted = [...RELEASES].sort((a, b) => compareSemver(b.version, a.version));
  return sorted[0];
}
