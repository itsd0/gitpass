import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { ItemList } from './ItemList';
import { ItemDetailPane } from './ItemDetail';
export function ItemsPage() {
  return (
    <PanelGroup direction="horizontal" autoSaveId={undefined}>
      <Panel defaultSize={34} minSize={22} maxSize={55}><ItemList /></Panel>
      <PanelResizeHandle className="w-px bg-border transition-colors hover:bg-accent data-[resize-handle-state=drag]:bg-accent" />
      <Panel minSize={35}><div className="h-full bg-bg"><ItemDetailPane /></div></Panel>
    </PanelGroup>
  );
}
