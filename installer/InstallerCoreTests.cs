using System;
using System.IO;
using System.Text;

namespace SnowDown.Setup
{
    internal static class InstallerCoreTests
    {
        private static int assertions;
        private static void Check(bool valid, string label)
        {
            if (!valid) throw new Exception("FAILED: " + label);
            assertions++;
        }
        private static void Reject(string path)
        {
            try { InstallerCore.NormalizePath(path); throw new Exception("Accepted invalid path: " + path); }
            catch (ArgumentException) { assertions++; }
        }
        private static int Main()
        {
            Check(InstallerCore.NormalizePath(@"D:\应用 文件\SnowDown\") == @"D:\应用 文件\SnowDown", "Unicode path");
            Check(InstallerCore.BackendArguments(@"D:\应用 文件\SnowDown") == @"/S /D=D:\应用 文件\SnowDown", "NSIS last unquoted directory");
            Reject(@"D:\"); Reject(@"D:relative"); Reject("relative"); Reject(@"\\server\share\SnowDown");
            Reject("D:\\Apps\\SnowDown\" /R"); Reject("D:\\Apps\\SnowDown\n/R");
            using (MemoryStream bytes = new MemoryStream(Encoding.UTF8.GetBytes("abc")))
                Check(InstallerCore.Hash(bytes) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "SHA256 payload integrity");
            Check(InstallerCore.StageText("running").Contains("退出"), "busy app message");
            Check(InstallerCore.StageText("files").Contains("音视频"), "real backend phase");
            string temporary = Path.Combine(Path.GetTempPath(), "SnowDown-CoreTest-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(temporary);
            InstallerCore.Cleanup(temporary);
            Check(Directory.Exists(temporary), "cleanup refuses a directory it did not create");
            Directory.Delete(temporary);
            Console.WriteLine("Installer core: " + assertions + " assertions passed.");
            return 0;
        }
    }
}
