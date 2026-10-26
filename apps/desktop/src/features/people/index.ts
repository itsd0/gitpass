import { Users } from 'lucide-react';
import { PeoplePage } from './PeoplePage';
import { ShareDialogHost } from './ShareDialog';
export { ShareDialog, ShareDialogHost } from './ShareDialog';
export const feature = { id: 'people', route: 'people', sidebar: { label: 'nav.people', icon: Users, section: 'top' as const }, Page: PeoplePage, Modal: ShareDialogHost };
