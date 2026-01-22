import { Composition, Folder } from "remotion";
import { DemoVideo } from "./DemoVideo";
import { LocaleKey } from "./locales";

const languages: { key: LocaleKey; name: string; folder: string }[] = [
  { key: "en", name: "English", folder: "English" },
  { key: "zh-TW", name: "繁體中文", folder: "Traditional-Chinese" },
  { key: "zh-CN", name: "简体中文", folder: "Simplified-Chinese" },
  { key: "ja", name: "日本語", folder: "Japanese" },
  { key: "es", name: "Español", folder: "Spanish" },
  { key: "fr", name: "Français", folder: "French" },
];

export const RemotionRoot = () => {
  return (
    <>
      {languages.map((lang) => (
        <Folder key={lang.key} name={lang.folder}>
          <Composition
            id={`${lang.key}-demo`}
            component={DemoVideo}
            durationInFrames={1200}
            fps={30}
            width={1920}
            height={1080}
            defaultProps={{
              locale: lang.key,
            }}
          />
        </Folder>
      ))}
    </>
  );
};
