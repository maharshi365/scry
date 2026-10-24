# Dot-source this file to inspect and exercise Scry's actual Windows UI.
# Captures only the selected app window, never the desktop. Coordinates are
# physical client pixels. Keyboard input requires Scry to be foreground.
Add-Type -AssemblyName System.Drawing
if (-not ('ScryNativeUi' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class ScryNativeUi {
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
    [StructLayout(LayoutKind.Sequential)] struct Mouse { public int x,y; public uint data,flags,time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] struct Keyboard { public ushort key,scan; public uint flags,time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Explicit)] struct Payload { [FieldOffset(0)] public Mouse mouse; [FieldOffset(0)] public Keyboard keyboard; }
    [StructLayout(LayoutKind.Sequential)] struct Input { public uint type; public Payload payload; }
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback,IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint id);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window,StringBuilder text,int count);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window,out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window,ref Point point);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint from,uint to,bool attach);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr window);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window,int command);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window,IntPtr dc,uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window,uint message,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern uint SendInput(uint count,Input[] inputs,int size);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern short VkKeyScan(char character);
    public static IntPtr Find(uint process) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((window,data)=> {
            uint id; GetWindowThreadProcessId(window,out id);
            var title=new StringBuilder(256); GetWindowText(window,title,256);
            if(id==process && IsWindowVisible(window) && title.ToString().StartsWith("Skry",StringComparison.OrdinalIgnoreCase)) { found=window; return false; }
            return true;
        },IntPtr.Zero);
        return found;
    }
    public static bool Activate(IntPtr window) {
        uint ignored; uint current=GetCurrentThreadId();
        uint foreground=GetWindowThreadProcessId(GetForegroundWindow(),out ignored);
        uint target=GetWindowThreadProcessId(window,out ignored);
        bool a=current!=foreground && AttachThreadInput(current,foreground,true);
        bool b=current!=target && AttachThreadInput(current,target,true);
        try { ShowWindow(window,9); BringWindowToTop(window); SetForegroundWindow(window); }
        finally { if(b) AttachThreadInput(current,target,false); if(a) AttachThreadInput(current,foreground,false); }
        System.Threading.Thread.Sleep(80);
        return GetForegroundWindow()==window;
    }
    static IntPtr Position(int x,int y) { return new IntPtr((x & 65535) | ((y & 65535)<<16)); }
    static void MoveTo(IntPtr window,int x,int y) {
        if(!Activate(window)) throw new InvalidOperationException("Scry is not foreground; mouse input was not sent");
        var point=new Point { X=x,Y=y }; ClientToScreen(window,ref point);
        if(!SetCursorPos(point.X,point.Y)) throw new InvalidOperationException("Cannot position native pointer");
        System.Threading.Thread.Sleep(40);
    }
    static void MouseInput(uint flags,int data=0) {
        var input=new Input { type=0,payload=new Payload { mouse=new Mouse { flags=flags,data=unchecked((uint)data) } } };
        if(SendInput(1,new[] { input },Marshal.SizeOf<Input>())!=1) throw new InvalidOperationException("Native pointer dispatch failed");
    }
    public static void Click(IntPtr window,int x,int y) {
        MoveTo(window,x,y); MouseInput(2); MouseInput(4);
    }
    public static void Wheel(IntPtr window,int x,int y,int delta) {
        MoveTo(window,x,y); MouseInput(0x800,delta);
    }
    static void Key(ushort key,ushort scan,uint flags) {
        var input=new Input { type=1,payload=new Payload { keyboard=new Keyboard { key=key,scan=scan,flags=flags } } };
        if(SendInput(1,new[] { input },Marshal.SizeOf<Input>())!=1) throw new InvalidOperationException("Native keyboard dispatch failed");
    }
    public static void Type(string text) {
        foreach(char c in text) {
            short mapped=VkKeyScan(c);
            if(mapped==-1) { Key(0,c,4); Key(0,c,6); }
            else {
                bool shift=(mapped & 256)!=0;
                if(shift) Key(16,0,0);
                Press((ushort)(mapped & 255));
                if(shift) Key(16,0,2);
            }
            System.Threading.Thread.Sleep(8);
        }
    }
    public static void Press(ushort key) { Key(key,0,0); Key(key,0,2); }
    public static void Shortcut(ushort modifier,ushort key) { Key(modifier,0,0); Press(key); Key(modifier,0,2); }
}
'@
}

function Get-ScryWindow([int]$ProcessId) {
    $window = [ScryNativeUi]::Find($ProcessId)
    if ($window -eq [IntPtr]::Zero) { throw "Scry window not found for PID $ProcessId" }
    return $window
}

function Save-ScryWindow([IntPtr]$Window, [string]$Path) {
    $rect = [ScryNativeUi+Rect]::new()
    if (-not [ScryNativeUi]::GetClientRect($Window, [ref]$rect)) { throw 'Cannot read Scry client size' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right, $rect.Bottom)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [ScryNativeUi]::PrintWindow($Window, $dc, 3)) { throw 'Window capture failed; no desktop fallback was used' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try { $bitmap.Save([IO.Path]::GetFullPath($Path), [Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
}

function Focus-ScryWindow([IntPtr]$Window) {
    if (-not [ScryNativeUi]::Activate($Window)) {
        throw 'Scry is not foreground; keyboard input was not sent'
    }
}

function Write-ScryText([IntPtr]$Window, [string]$Text) {
    Focus-ScryWindow $Window
    [ScryNativeUi]::Type($Text)
}
