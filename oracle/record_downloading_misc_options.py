#!/usr/bin/env python3
"""Record the downloading "misc" options on the real reference code they steer.

- show N / show D on the short file import summary: FileSeedCache status text
  (simple) for many mixes of statuses under each pair of options;
- consider %20 the same as a space: GalleryURLGenerator.GenerateGalleryURL;
- remove leading double slashes: ConvertPathTextToList and a URL class
  matching and normalising URLs with a double slash.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    c=session.controller
    def work():
        from hydrus.client import ClientConstants as CC
        from hydrus.client.importing import ClientImportFileSeeds as S
        from hydrus.client.networking import ClientNetworkingGUG as G, ClientNetworkingFunctions as F, ClientNetworkingURLClass as U
        from hydrus.client import ClientStrings as CS
        o=c.new_options
        old={k:o.GetBoolean(k) for k in ('show_new_on_file_seed_short_summary','show_deleted_on_file_seed_short_summary','replace_percent_twenty_with_space_in_gug_input','remove_leading_url_double_slashes')}
        out={'summaries':[],'gug':[],'paths':[],'url_class':[]}
        names={'new':CC.STATUS_SUCCESSFUL_AND_NEW,'redundant':CC.STATUS_SUCCESSFUL_BUT_REDUNDANT,'ignored':CC.STATUS_VETOED,'deleted':CC.STATUS_DELETED,'failed':CC.STATUS_ERROR,'skipped':CC.STATUS_SKIPPED,'unknown':CC.STATUS_UNKNOWN}
        mixes=[{},{'unknown':3},{'new':2},{'deleted':1},{'new':2,'deleted':1},{'new':2,'deleted':1,'unknown':4},{'redundant':3,'ignored':1,'failed':2,'skipped':1},
               {'new':1200,'deleted':1500,'ignored':2,'failed':3,'skipped':4,'unknown':5},{'deleted':2,'ignored':1},{'new':1,'redundant':1,'deleted':1,'ignored':1,'failed':1,'skipped':1,'unknown':1}]
        for mix in mixes:
            seeds=[];n=0
            for name,count in mix.items():
                for _ in range(count):
                    n+=1;s=S.FileSeed(S.FILE_SEED_TYPE_URL,f'https://example.com/post/{n}')
                    if names[name]!=CC.STATUS_UNKNOWN:s.SetStatus(names[name])
                    seeds.append(s)
            cache=S.FileSeedCache();cache.AddFileSeeds(seeds)
            for show_new in (False,True):
                for show_deleted in (False,True):
                    o.SetBoolean('show_new_on_file_seed_short_summary',show_new);o.SetBoolean('show_deleted_on_file_seed_short_summary',show_deleted)
                    out['summaries'].append({'mix':mix,'show_new':show_new,'show_deleted':show_deleted,'text':cache.GetStatus().GetStatusText(simple=True)})
        for template,label in (('https://booru.example/posts?tags=%tags%','params'),('https://booru.example/artist/%tags%/list','path')):
            gug=G.GalleryURLGenerator('misc',url_template=template,replacement_phrase='%tags%',search_terms_separator='+',example_search_text='blue')
            for query in ('blue%20eyes','blue eyes','100%20%','a%2520b','%20lead','x%20%20y'):
                for on in (False,True):
                    o.SetBoolean('replace_percent_twenty_with_space_in_gug_input',on)
                    out['gug'].append({'template':label,'query':query,'on':on,'url':gug.GenerateGalleryURL(query)})
        for path in ('//a//b','/a','a','///x/y','/','//'):
            for on in (False,True):
                o.SetBoolean('remove_leading_url_double_slashes',on)
                out['paths'].append({'path':path,'on':on,'components':F.ConvertPathTextToList(path)})
        import hydrus.core.HydrusConstants as HC
        cls=U.URLClass('images',url_type=HC.URL_TYPE_POST,parameters=[],url_domain_mask=U.URLDomainMask(raw_domains=['booru.example']),path_components=[(CS.StringMatch(match_type=CS.STRING_MATCH_FIXED,match_value='images'),None),(CS.StringMatch(),None)],example_url='https://booru.example/images/abc.jpg')
        for url in ('https://booru.example/images/abc.jpg','https://booru.example//images/abc.jpg','https://booru.example///images/abc.jpg','https://booru.example//images//abc.jpg'):
            for on in (False,True):
                o.SetBoolean('remove_leading_url_double_slashes',on)
                row={'url':url,'on':on}
                try:
                    cls.Test(url);row['matches']=True
                except Exception as e:row['matches']=False;row['why']=str(e)
                try:row['normalised']=cls.Normalise(url)
                except Exception as e:row['normalised_error']=type(e).__name__
                out['url_class'].append(row)
        for k,v in old.items():o.SetBoolean(k,v)
        return out
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures','downloading_misc_options.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded downloading misc options')
if __name__=='__main__':main()
