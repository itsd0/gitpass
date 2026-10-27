import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { ItemList } from './ItemList';
import { ItemDetailPane } from './ItemDetail';
import { BulkConfirm, BulkPane } from './BulkPane';
import { useApp } from '../../lib/store';
export function ItemsPage() {
  const multi = useApp((s) => s.multiIds.length > 1);
  return (
    <>
      <PanelGroup direction="horizontal" autoSaveId={undefined}>
        <Panel defaultSize={34} minSize={22} maxSize={55}><ItemList /></Panel>
        <PanelResizeHandle className="w-px bg-border transition-colors hover:bg-accent data-[resize-handle-state=drag]:bg-accent" />
        <Panel minSize={35}><div className="h-full bg-bg">{multi ? <BulkPane /> : <ItemDetailPane />}</div></Panel>
      </PanelGroup>
      <BulkConfirm />
    </>
  );
}
