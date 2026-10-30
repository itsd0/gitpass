import { FileText, Globe, PanelLeftClose, ShieldAlert, ShieldCheck } from 'lucide-react';
import type { ReleaseHighlights } from './types';
import { compareSemver } from './semver';

export const RELEASES: ReleaseHighlights[] = [
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
