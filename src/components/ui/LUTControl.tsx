import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { X, Folder, ChevronDown, ChevronUp } from 'lucide-react';
import Slider from './Slider';
import { useState, useEffect } from 'react';

interface LutFileInfo {
  name: string;
  path: string;
}

interface LUTControlProps {
  lutName: string | null;
  lutIntensity: number;
  onLutSelect: (path: string) => void;
  onIntensityChange: (intensity: number) => void;
  onClear: () => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

export default function LUTControl({
  lutName,
  lutIntensity,
  onLutSelect,
  onIntensityChange,
  onClear,
  onDragStateChange,
}: LUTControlProps) {
  const [lutFolder, setLutFolder] = useState<string | null>(null);
  const [lutFiles, setLutFiles] = useState<LutFileInfo[]>([]);
  const [showLutList, setShowLutList] = useState(false);

  const handleSelectFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'LUT Files',
            extensions: ['cube', '3dl'],
          },
          {
            name: 'HALD Images',
            extensions: ['png', 'jpg', 'jpeg', 'tiff'],
          },
        ],
      });
      if (typeof selected === 'string') {
        onLutSelect(selected);
      }
    } catch (err) {
      console.error('Failed to open LUT file dialog:', err);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (typeof selected === 'string') {
        setLutFolder(selected);
        await loadLutFiles(selected);
        setShowLutList(true);
      }
    } catch (err) {
      console.error('Failed to open folder dialog:', err);
    }
  };

  const loadLutFiles = async (folderPath: string) => {
    try {
      const files: LutFileInfo[] = await invoke('list_lut_files', { dirPath: folderPath });
      setLutFiles(files);
    } catch (err) {
      console.error('Failed to list LUT files:', err);
      setLutFiles([]);
    }
  };

  const handleLutFileClick = (path: string) => {
    onLutSelect(path);
  };

  const clearFolder = () => {
    setLutFolder(null);
    setLutFiles([]);
    setShowLutList(false);
  };

  return (
    <div className="mb-2">
      <div className="flex justify-between items-center mb-1">
        <span className="text-sm font-medium text-text-secondary select-none">LUT</span>
        <div className="group flex items-center gap-1">
          <button
            onClick={handleSelectFile}
            className="text-sm text-text-primary text-right select-none cursor-pointer truncate max-w-[120px] hover:text-accent transition-colors"
            title={lutName || 'Select a LUT file'}
          >
            {lutName || 'Select File'}
          </button>

          <button
            onClick={handleSelectFolder}
            className="flex items-center justify-center p-1 rounded hover:bg-surface transition-colors"
            title="Select LUT folder"
          >
            <Folder size={14} />
          </button>

          {lutName && (
            <button
              onClick={onClear}
              className="flex items-center justify-center p-0.5 rounded-full bg-bg-tertiary hover:bg-surface 
                         w-0 ml-0 opacity-0 group-hover:w-6 group-hover:ml-0 group-hover:opacity-100 
                         overflow-hidden pointer-events-none group-hover:pointer-events-auto
                         transition-all duration-200 ease-in-out"
              title="Clear LUT"
            >
              <X size={14} />
            </button>
          )}
        </div>
      </div>

      {lutFolder && (
        <div className="mb-2">
          <div className="flex justify-between items-center">
            <div className="flex items-center gap-1">
              <button
                onClick={() => setShowLutList(!showLutList)}
                className="flex items-center gap-1 text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                {showLutList ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                <span className="truncate max-w-[120px]" title={lutFolder}>
                  {lutFolder.split(/[\\/]/).pop()}
                </span>
                <span className="text-text-tertiary">({lutFiles.length})</span>
              </button>
            </div>
            <button
              onClick={clearFolder}
              className="text-xs text-text-tertiary hover:text-text-secondary transition-colors"
              title="Clear folder"
            >
              <X size={12} />
            </button>
          </div>

          {showLutList && lutFiles.length > 0 && (
            <div className="mt-1 max-h-40 overflow-y-auto bg-bg-secondary rounded border border-surface p-1">
              {lutFiles.map((file) => (
                <button
                  key={file.path}
                  onClick={() => handleLutFileClick(file.path)}
                  className={`w-full text-left text-xs px-2 py-1 rounded hover:bg-surface transition-colors truncate ${
                    lutName === file.name ? 'bg-surface text-accent' : 'text-text-primary'
                  }`}
                  title={file.name}
                >
                  {file.name}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {lutName && (
        <Slider
          label="Intensity"
          min={0}
          max={100}
          step={1}
          value={lutIntensity}
          defaultValue={100}
          onChange={(e) => onIntensityChange(parseInt(e.target.value, 10))}
          onDragStateChange={onDragStateChange}
        />
      )}
    </div>
  );
}
