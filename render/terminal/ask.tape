Type "ask --url $url 'train 41: 6 branches, gate green' 'Is this train ready to merge?'"
Enter
Wait@20s /^\$\s*$/
Sleep 1500ms
Type "ask --url $url 'train 41: 6 branches, gate green' 'Which track?' fast='the short gate' full='every suite'"
Enter
Wait@20s /^\$\s*$/
Sleep 1500ms
Type "JEV_API_KEY=wrong-key ask --url $url 'train 41' 'Ready?'"
Enter
Wait@20s /^\$\s*$/
Sleep 3s
