using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;

namespace VoicePaste.Settings;

/// <summary>Standard Windows color chooser, owned by Settings; no WinForms dependency required.</summary>
internal static class NativeColorPicker
{
    private static readonly int[] CustomColors = Enumerable.Repeat(0xFFFFFF, 16).ToArray();

    public static bool TryChoose(Window owner, Color initial, out Color selected)
    {
        selected = initial;
        var colors = GCHandle.Alloc(CustomColors, GCHandleType.Pinned);
        try
        {
            var data = new ChooseColorData
            {
                Size = Marshal.SizeOf<ChooseColorData>(),
                Owner = new WindowInteropHelper(owner).Handle,
                Color = ToColorRef(initial),
                CustomColors = colors.AddrOfPinnedObject(),
                Flags = 0x00000001 | 0x00000002 // CC_RGBINIT | CC_FULLOPEN
            };
            if (!ChooseColor(ref data))
            {
                var error = CommDlgExtendedError();
                if (error != 0) throw new InvalidOperationException($"Windows color chooser failed (0x{error:X}).");
                return false;
            }
            selected = FromColorRef(data.Color);
            return true;
        }
        finally { colors.Free(); }
    }

    internal static uint ToColorRef(Color color) => (uint)(color.R | color.G << 8 | color.B << 16);
    internal static Color FromColorRef(uint value) => Color.FromRgb((byte)value, (byte)(value >> 8), (byte)(value >> 16));

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ChooseColorData
    {
        public int Size;
        public IntPtr Owner;
        public IntPtr Instance;
        public uint Color;
        public IntPtr CustomColors;
        public uint Flags;
        public IntPtr CustomData;
        public IntPtr Hook;
        public IntPtr TemplateName;
    }

    [DllImport("comdlg32.dll", EntryPoint = "ChooseColorW", CharSet = CharSet.Unicode, ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ChooseColor(ref ChooseColorData data);

    [DllImport("comdlg32.dll", ExactSpelling = true)]
    private static extern uint CommDlgExtendedError();
}
